Sovereign Agent Runtime: Comprehensive Technical Documentation

Version: 3.0 (Foundation Release)
Status: Core Infrastructure Hardened; Cognitive/Compute Layers are Stubs

1. Project Status and Validation Summary

This document reflects the validated, current state of the Sovereign Agent Runtime v3.0 project. A comprehensive audit has confirmed that the core infrastructure components are robust, secure, and ready for further development. However, the higher-level sovereign computing layers remain as foundational stubs.

✅ Production-Ready Infrastructure:

sovereign-protocol: The IPC contract is stable and complete.

sovereign-mesh: The "Dark Mesh" networking layer is hardened with PNet-first security and is fully functional.

sovereign-finance: The Bitcoin license verification engine is cryptographically sound and production-ready.

sovereign-node: The central coordinator is resilient, employing a robust actor model and correct handling of blocking tasks.

⚠️ Stub/Framework Components:

sovereign-core: The architectural framework for the cognitive layer exists, but the full integration and execution of Datalog/CozoDB reasoning is a pending task.

sovereign-runtime-wasm: The secure sandbox environment using Wasmtime is established, but the logic for executing arbitrary WASM modules is a pending task.

❌ Missing Components:

sovereign-cli: The command-line interface, while defined by the protocol, has not yet been implemented.

The project successfully builds and represents a solid foundation for a sovereign agent architecture but is not yet feature-complete.

2. Vision & Architecture
2.1 Project Philosophy

The Sovereign Agent Runtime is an architectural blueprint for a new class of personal software. It rejects the "cloud-tethered" model in favor of a Local Sovereign Engine: an OS-adjacent runtime that transforms a user's machine into a self-contained, autonomous node capable of reasoning, secure networking, and economic agency.

Core Tenets:

Sovereignty: The user owns and controls the software, data, and logic.

Privacy via "Dark Mesh": Network participation is restricted to authorized devices, rendering the agent invisible to the public internet.

Deterministic Cognition: The agent is designed to use deductive logic (Datalog) for predictable and auditable behavior.

Economic Reality: The agent verifies licenses and contracts directly against the Bitcoin blockchain.

2.2 System Architecture

The runtime is architected as a Rust Workspace composed of multiple, strictly decoupled crates. The system runs as a persistent background process, acting as a "Hypervisor for Logic" that orchestrates specialized subsystems (Actors).

Layer	Crate	Responsibility	Status
Coordinator	sovereign-node	Process lifecycle, IPC, Actor supervision.	✅ Implemented
Protocol	sovereign-protocol	Shared types, IPC contract, Serialization.	✅ Implemented
Cognition	sovereign-core	Framework for long-term memory & reasoning.	⚠️ Stub
Compute	sovereign-runtime-wasm	Framework for secure execution of skills.	⚠️ Stub
Network	sovereign-mesh	Private P2P networking ("Dark Mesh").	✅ Implemented
Finance	sovereign-finance	Blockchain verification & economics.	✅ Implemented
Client	sovereign-cli	User-facing control plane.	❌ Not Implemented

3. Crate-by-Crate Breakdown
3.1 sovereign-protocol (The API Contract)

This crate defines the unified "Lingua Franca" of the system. It contains the Request and Response enums for all Inter-Process Communication (IPC).

Design: Strictly typed enums with full serde derivation ensure the client and server protocols remain in sync, preventing "poison pill" payloads.

Observability: The NodeStatus struct aggregates key health metrics (peer ID, connection count, license status) into a single heartbeat object.

3.2 sovereign-mesh (The Dark Mesh)

This crate implements the private networking layer, designed to be invisible to standard network scanners.

Transport Pipeline (Hardened): The implementation manually constructs the libp2p transport stack to enforce a PNet-first security model:

TCP: Establishes the raw socket.

PNet Handshake: Immediately upgrades the connection using a Pre-Shared Key. Unauthorized peers are dropped here, before their PeerId is exchanged.

Noise Upgrade: Encrypts the authenticated session.

Yamux: Multiplexes streams over the single, secure connection.

Swarm Key: The system correctly parses standard IPFS-format swarm.key files, allowing interoperability with existing P2P tooling.

3.3 sovereign-finance (Economic Verification)

This crate gives the agent the ability to verify financial facts on the Bitcoin blockchain.

Verification Logic (Hardened): A valid license requires a transaction that satisfies a strict logical AND condition:

Payment Proof: An output exists paying >= required_sats to the developer_address. This is verified by a byte-for-byte comparison of the transaction's script_pubkey.

Binding Proof: An OP_RETURN output exists with a data payload that is a byte-for-byte match for SHA256("LICENSE" + machine_id).

Implementation: It uses bdk with an ElectrumBlockchain backend for SPV-like verification. Its functions are synchronous and designed to be run in a blocking thread pool.

3.4 sovereign-node (The Coordinator)

The central process that orchestrates all other subsystems.

Identity: On startup, it derives a stable MachineID using the machine-uid crate, ensuring licenses remain bound to the hardware.

Actor Model & Concurrency:

It spawns the MeshNode as a background Tokio task, communicating with it via non-blocking channels.

It uses tokio::task::spawn_blocking to run all potentially blocking workloads (database queries, WASM execution, and Bitcoin verification). This is key to the runtime's resilience and responsiveness.

IPC Loop: Listens for client connections on the platform-native socket, processing length-prefixed JSON requests asynchronously.

4. Operational Procedures
4.1 Building the Runtime

The project is a Cargo Workspace. Build all components in release mode:

code
Bash
download
content_copy
expand_less
cargo build --workspace --release

4.2 Configuration

The agent requires a swarm.key file for the private mesh, placed in the same directory as the executable.

File: swarm.key

code
Code
download
content_copy
expand_less
/key/swarm/psk/1.0.0/
/base16/
<YOUR_64_CHARACTER_HEXADECIMAL_KEY_HERE>

4.3 Installation and Execution

The agent is designed to run as a persistent background process.

On Windows (as a Service):

code
Powershell
download
content_copy
expand_less
# Run as Administrator
sc.exe create SovereignNode binPath="C:\Path\To\target\release\sovereign-node.exe" start=auto
sc.exe start SovereignNode

On macOS/Linux (as a Daemon):

code
Bash
download
content_copy
expand_less
# Run in the background using nohup
nohup /path/to/target/release/sovereign-node &
# For production, a systemd or launchd service file is recommended.

5. Security Model
5.1 Data Privacy

In Transit: All mesh traffic is protected by two layers of encryption: PNet (XSalsa20) as a gatekeeper and Noise (ChaCha20-Poly1505) for the session. This model ensures that both the content and metadata (like PeerId) of communications are kept private.

At Rest: The architecture supports swapping the standard SQLite backend in sovereign-core for SQLCipher to enable transparent, full-database AES encryption.

5.2 Execution Security

Sandboxing: The sovereign-runtime-wasm crate establishes a Wasmtime sandbox. When fully implemented, it will ensure that third-party skills have no access to the host filesystem, network, or memory by default.

Resource Management: The Wasmtime engine can be configured with resource limits (e.g., memory, CPU fuel) to mitigate denial-of-service attacks from runaway skill modules.

5.3 Economic Security

Trustless Verification: License verification relies solely on the Bitcoin Blockchain's public ledger. The agent trusts cryptographic proof and network consensus, not a central license server.

Hardware Binding: The license is cryptographically bound to the machine's stable hardware ID via the OP_RETURN hash. This prevents casual license copying while preserving user privacy, as the raw hardware ID is never exposed on-chain.
