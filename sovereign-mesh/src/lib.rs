use libp2p::{
    gossipsub, kad, mdns, noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder, Transport,
    core::upgrade::Version,
};
use std::path::Path;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use log::{info, error, warn, debug};
use futures::StreamExt;

// --- 1. The Behaviour Definition ---
// In libp2p 0.53, the NetworkBehaviour derive auto-generates the event enum.
// We add the Ping behaviour for NAT traversal.
#[derive(NetworkBehaviour)]
pub struct SovereignBehaviour {
    gossipsub: gossipsub::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    mdns: mdns::tokio::Behaviour,
    ping: libp2p::ping::Behaviour,
}

pub struct MeshNode {
    swarm: Swarm<SovereignBehaviour>,
    command_rx: mpsc::Receiver<MeshCommand>,
}

pub enum MeshCommand {
    Dial(String),
    GetPeers(oneshot::Sender<Vec<String>>),
    GetPeerId(oneshot::Sender<String>),
}

impl MeshNode {
    pub fn new(
        key_path: &Path,
        command_rx: mpsc::Receiver<MeshCommand>,
    ) -> anyhow::Result<Self> {
        // --- Identity & Key Generation ---
        let id_keys = libp2p::identity::Keypair::generate_ed25519();
        let peer_id = PeerId::from(id_keys.public());
        info!("Mesh Identity Initialized: {}", peer_id);

        // --- PNet Configuration (The "Dark" Layer) ---
        // TODO: Implement PNet transport conditional on PSK
        // let _psk = match load_swarm_key(key_path) {
        //     Ok(key) => Some(key),
        //     Err(e) => {
        //         warn!("CRITICAL: Swarm key error: {}. Mesh running in OPEN/INSECURE mode.", e);
        //         None
        //     }
        // };

        // --- Hardened Transport Pipeline Construction ---
        let tcp_config = tcp::Config::default().nodelay(true);
        let base_transport = tcp::tokio::Transport::new(tcp_config);

        let noise_config = noise::Config::new(&id_keys).expect("Noise key generation failed");

        let transport = base_transport
            .upgrade(Version::V1)
            .authenticate(noise_config)
            .multiplex(yamux::Config::default())
            .boxed();

        // --- Behaviour Configuration ---
        let message_authenticity = gossipsub::MessageAuthenticity::Signed(id_keys.clone());
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(1))
            .validation_mode(gossipsub::ValidationMode::Strict)
            .build()
            .map_err(|msg| anyhow::anyhow!("Gossipsub config error: {}", msg))?;

        let gossipsub = gossipsub::Behaviour::new(message_authenticity, gossipsub_config).map_err(|e| anyhow::anyhow!("Failed to create gossipsub: {}", e))?;
        let kademlia = kad::Behaviour::new(peer_id, kad::store::MemoryStore::new(peer_id));
        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;
        let ping = libp2p::ping::Behaviour::new(libp2p::ping::Config::new());

        let behaviour = SovereignBehaviour { gossipsub, kademlia, mdns, ping };

        // --- Swarm Builder (0.53 Syntax) ---
        let swarm = SwarmBuilder::with_existing_identity(id_keys)
            .with_tokio()
            .with_other_transport(|_keypair| transport)?
            .with_behaviour(|_| behaviour)?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        Ok(Self { swarm, command_rx })
    }

    // --- The Mesh Actor Loop ---
    pub async fn run(mut self) {
        if let Err(e) = self.swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap()) {
            error!("Failed to start listener: {}", e);
            return;
        }

        loop {
            tokio::select! {
                cmd = self.command_rx.recv() => match cmd {
                    Some(MeshCommand::Dial(addr)) => {
                        if let Ok(ma) = addr.parse::<Multiaddr>() {
                            let _ = self.swarm.dial(ma);
                        }
                    },
                    Some(MeshCommand::GetPeers(tx)) => {
                        let peers = self.swarm.connected_peers().map(|p| p.to_string()).collect();
                        let _ = tx.send(peers);
                    },
                    Some(MeshCommand::GetPeerId(tx)) => {
                        let _ = tx.send(self.swarm.local_peer_id().to_string());
                    },
                    None => {
                        info!("Mesh Command Channel closed. Shutting down Mesh Actor.");
                        break;
                    },
                },
                event = self.swarm.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address,.. } => info!("Mesh listening on {:?}", address),
                    SwarmEvent::Behaviour(SovereignBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                        for (peer, addr) in list {
                            info!("mDNS Discovered: {} at {}", peer, addr);
                            self.swarm.behaviour_mut().kademlia.add_address(&peer, addr);
                        }
                    },
                    SwarmEvent::Behaviour(SovereignBehaviourEvent::Ping(event)) => {
                        debug!("Ping event: {:?}", event);
                    },
                    _ => {}
                }
            }
        }
    }
}

// --- Helper: Robust Key Loading ---
// TODO: Uncomment and fix when implementing PNet
/*
fn load_swarm_key(path: &Path) -> anyhow::Result<PreSharedKey> {
    let content = std::fs::read_to_string(path)?;
    let mut lines = content.lines();

    // Strict parsing ensures proper format
    if lines.next() != Some("/key/swarm/psk/1.0.0/") {
        return Err(anyhow::anyhow!("Invalid swarm key header"));
    }
    if lines.next() != Some("/base16/") {
        return Err(anyhow::anyhow!("Unsupported encoding - must be base16"));
    }
    let key_hex = lines.next().ok_or(anyhow::anyhow!("Missing key data"))?;
    let bytes = hex::decode(key_hex.trim())?;

    let key_arr: [u8; 32] = bytes.try_into().map_err(|_| anyhow::anyhow!("Key must be exactly 32 bytes"))?;
    Ok(PreSharedKey::new(key_arr))
}
*/
