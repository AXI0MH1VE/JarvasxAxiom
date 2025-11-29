use std::sync::Arc;
use std::time::SystemTime;
use env_logger;
use tokio::sync::Mutex;
use sovereign_core::CognitiveCore;
use sovereign_runtime_wasm::WasmRuntime;

mod service_loop;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    // Note: This is adapted for macOS. Original uses Windows services.
    // For macOS, run as a regular process.

    let start_time = SystemTime::now();

    // Initialize core and wasm
    let core = Arc::new(Mutex::new(CognitiveCore::new()?));
    let wasm = WasmRuntime::new()?;

    service_loop::run_ipc_server(core, wasm, start_time).await
}
