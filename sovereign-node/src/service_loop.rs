use anyhow::Result;
use log::{info, error};
use sovereign_core::CognitiveCore;
use sovereign_finance::LicenseVerifier;
use sovereign_mesh::{MeshCommand, MeshNode};
use sovereign_protocol::{NodeStatus, Request, Response};
use sovereign_runtime_wasm::WasmRuntime;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, oneshot, Mutex};

struct SharedState {
    peer_id: String,
    connections: u32,
    license_active: bool,
}

pub async fn run_ipc_server(
    core: Arc<Mutex<CognitiveCore>>,
    wasm: Arc<WasmRuntime>,
    start_time: SystemTime,
) -> Result<()> {
    // 1. Hardware Identity
    let machine_id = machine_uid::get().unwrap_or_else(|_| "fallback-id".into());
    info!("Sovereign Agent ID: {}", machine_id);

    let state = Arc::new(RwLock::new(SharedState {
        peer_id: "Initializing...".into(),
        connections: 0,
        license_active: false,
    }));

    // 2. Start Mesh Actor
    let (mesh_tx, mesh_rx) = mpsc::channel(32);
    let key_path = std::path::Path::new("swarm.key");

    // Generate dev key if missing
    if !key_path.exists() {
        let dev_key = "/key/swarm/psk/1.0.0/\n/base16/\n0000000000000000000000000000000000000000000000000000000000000000";
        std::fs::write(key_path, dev_key).ok();
    }

    let mesh_node = MeshNode::new(key_path, mesh_rx)?;
    tokio::spawn(mesh_node.run());

    // Cache PeerID
    let (pid_tx, pid_rx) = oneshot::channel();
    let _ = mesh_tx.send(MeshCommand::GetPeerId(pid_tx)).await;
    if let Ok(pid) = pid_rx.await {
        if let Ok(mut s) = state.write() {
            s.peer_id = pid;
        }
    }

    // 3. Start Finance Actor (The Verifier)
    // We wrap it in Arc to share across threads.
    let finance = Arc::new(LicenseVerifier::new("ssl://electrum.blockstream.info:50002", "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh", 50000)?);

    // 4. IPC Loop using Unix socket on macOS
    let socket_path = "/tmp/sovereign-node.sock";
    let _ = std::fs::remove_file(socket_path); // Remove old socket if exists
    let listener = UnixListener::bind(socket_path)?;
    info!("IPC server listening on Unix socket: {}", socket_path);

    loop {
        let (mut stream, _) = listener.accept().await?;
        let core = core.clone();
        let wasm_clone = wasm.clone();
        let mesh = mesh_tx.clone();
        let finance = finance.clone();
        let state = state.clone();
        let m_id = machine_id.clone();
        let start = start_time;

        tokio::spawn(async move {
            let mut len_buf = [0u8; 4];
            loop {
                if stream.read_exact(&mut len_buf).await.is_err() {
                    break;
                }
                let len = u32::from_le_bytes(len_buf) as usize;

                // B. Security Check: Allocation Limit
                const MAX_IPC_MSG_SIZE: usize = 64 * 1024; // 64 KB Limit to prevent memory exhaustion
                if len > MAX_IPC_MSG_SIZE {
                    error!("Security Violation: IPC client requested {} bytes. Dropping.", len);
                    break;
                }

                let mut buf = vec![0u8; len];
                if stream.read_exact(&mut buf).await.is_err() {
                    break;
                }

                let req: Request = match serde_json::from_slice(&buf) {
                    Ok(r) => r,
                    Err(_) => continue,
                };

                let resp = match req {
                    Request::GetStatus => {
                        let s = state.read().unwrap();
                        Response::Status(NodeStatus {
                            uptime_ms: SystemTime::now().duration_since(start).unwrap().as_millis()
                                as u64,
                            mesh_peer_id: s.peer_id.clone(),
                            mesh_connections: s.connections,
                            license_active: s.license_active,
                            system_health: "OK".into(),
                        })
                    }
                    Request::QueryCore { query, params } => {
                        let mut c = core.lock().await;
                        match c.run(&query, params) {
                            Ok(val) => Response::CoreResult(val),
                            Err(e) => Response::Error(e.to_string()),
                        }
                    }
                    Request::RunWasm { path: _, input } => {
                        let wasm_for_task = wasm_clone.clone();
                        let res = tokio::task::spawn_blocking(move || {
                            wasm_for_task.run_module(&[], &input)
                        })
                        .await;
                        match res {
                            Ok(Ok(out)) => Response::WasmOutput(out),
                            Ok(Err(e)) => Response::Error(e.to_string()),
                            Err(e) => Response::Error(e.to_string()),
                        }
                    }
                    Request::MeshDial { addr } => {
                        let _ = mesh.send(MeshCommand::Dial(addr)).await;
                        Response::MeshGeneric("Dialing...".into())
                    }
                    Request::MeshPeers => {
                        let (tx, rx) = oneshot::channel();
                        let _ = mesh.send(MeshCommand::GetPeers(tx)).await;
                        match rx.await {
                            Ok(peers) => {
                                if let Ok(mut s) = state.write() {
                                    s.connections = peers.len() as u32;
                                }
                                Response::MeshGeneric(format!("{:?}", peers))
                            }
                            Err(_) => Response::Error("Mesh timeout".into()),
                        }
                    }
                    Request::VerifyLicense { tx_id, .. } => {
                        let f = finance.clone();
                        let s = state.clone();
                        let tid = tx_id.clone();
                        let mid = m_id.clone();

                        // CRITICAL: Move the blocking verification to a separate thread
                        let res = tokio::task::spawn_blocking(move || {
                            f.verify_license_sync(&tid, &mid)
                        })
                        .await;

                        match res {
                            Ok(Ok(valid)) => {
                                if let Ok(mut state_lock) = s.write() {
                                    state_lock.license_active = valid;
                                }
                                Response::LicenseResult { valid, details: if valid { "Active".into() } else { "Invalid".into() } }
                            },
                            Ok(Err(e)) => Response::Error(format!("Verification Logic Failed: {}", e)),
                            Err(e) => Response::Error(format!("Task Panicked: {}", e)),
                        }
                    }
                    _ => Response::Pong, // Default response
                };

                let bytes = serde_json::to_vec(&resp).unwrap();
                let len_bytes = (bytes.len() as u32).to_le_bytes();
                stream.write_all(&len_bytes).await.ok();
                stream.write_all(&bytes).await.ok();
            }
        });
    }
}
