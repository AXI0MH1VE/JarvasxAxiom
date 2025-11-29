use serde::{Deserialize, Serialize};

/// The Windows Named Pipe address for IPC.
pub const PIPE_NAME: &str = r"\\.\pipe\SovereignNode";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Request {
    Ping,
    GetStatus,
    /// Execute a Datalog query (Cognitive Layer)
    QueryCore {
        query: String,
        params: serde_json::Value,
    },
    /// Execute a WASM module (Compute Layer)
    RunWasm {
        path: String,
        input: String,
    },
    /// Mesh: Connect to a specific peer
    MeshDial {
        addr: String,
    },
    /// Mesh: List active connections
    MeshPeers,
    /// Finance: Check for a valid license on-chain
    VerifyLicense {
        tx_id: String,
        developer_addr: String,
        required_sats: u64,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Response {
    Pong,
    Status(NodeStatus),
    CoreResult(serde_json::Value),
    WasmOutput(String),
    MeshGeneric(String),
    LicenseResult {
        valid: bool,
        details: String,
    },
    Error(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeStatus {
    pub uptime_ms: u64,
    pub mesh_peer_id: String,
    pub mesh_connections: u32,
    pub license_active: bool,
    pub system_health: String,
}
