use futures::{future::Either, StreamExt};
use libp2p::{
    gossipsub, kad, mdns, noise,
    pnet::{PnetConfig, PreSharedKey},
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder, Transport,
};
use std::path::Path;
use tokio::sync::{mpsc, oneshot};
use log::{info, error, warn};

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "SovereignBehaviourEvent")]
struct SovereignBehaviour {
    gossipsub: gossipsub::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>,
    mdns: mdns::tokio::Behaviour,
}

#[derive(Debug)]
pub enum SovereignBehaviourEvent {
    Gossipsub(gossipsub::Event),
    Kademlia(kad::Event),
    Mdns(mdns::Event),
}

impl From<gossipsub::Event> for SovereignBehaviourEvent {
    fn from(event: gossipsub::Event) -> Self { Self::Gossipsub(event) }
}
impl From<kad::Event> for SovereignBehaviourEvent {
    fn from(event: kad::Event) -> Self { Self::Kademlia(event) }
}
impl From<mdns::Event> for SovereignBehaviourEvent {
    fn from(event: mdns::Event) -> Self { Self::Mdns(event) }
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
        let id_keys = libp2p::identity::Keypair::generate_ed25519();
        let peer_id = PeerId::from(id_keys.public());
        info!("Mesh Identity Initialized: {}", peer_id);

        let psk = load_swarm_key(key_path).map_err(|e| {
            warn!("Swarm key error: {}. Mesh is OPEN/UNSECURE.", e);
            e
        }).ok();

        // Build transport with PNet conditionally
        let tcp_config = tcp::Config::default().nodelay(true);
        let noise_config = noise::Config::new(&id_keys).expect("Noise key gen failed");

        let tcp_transport = tcp::tokio::Transport::new(tcp_config);
        let transport = if let Some(key) = psk {
            tcp_transport
                .and_then(move |socket, _| Either::Left(PnetConfig::new(key).handshake(socket)))
                .upgrade(libp2p::core::upgrade::Version::V1)
                .authenticate(noise_config)
                .multiplex(yamux::Config::default())
                .boxed()
        } else {
            tcp_transport
                .upgrade(libp2p::core::upgrade::Version::V1)
                .authenticate(noise_config)
                .multiplex(yamux::Config::default())
                .boxed()
        };

        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(id_keys.clone()),
            gossipsub::Config::default(),
        ).map_err(anyhow::Error::msg)?;

        let kademlia = kad::Behaviour::new(peer_id, kad::store::MemoryStore::new(peer_id));
        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;
        let behaviour = SovereignBehaviour { gossipsub, kademlia, mdns };

        // Use legacy Swarm constructor since we have custom transport
        let swarm = Swarm::new(transport, behaviour, peer_id);
        let mut swarm = swarm;
        swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
        Ok(Self { swarm, command_rx })
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                cmd = self.command_rx.recv() => match cmd {
                    Some(MeshCommand::Dial(addr)) => {
                        if let Ok(ma) = addr.parse::<Multiaddr>() { let _ = self.swarm.dial(ma); }
                    },
                    Some(MeshCommand::GetPeers(tx)) => {
                        let peers = self.swarm.connected_peers().map(|p| p.to_string()).collect();
                        let _ = tx.send(peers);
                    },
                    Some(MeshCommand::GetPeerId(tx)) => {
                        let _ = tx.send(self.swarm.local_peer_id().to_string());
                    },
                    None => break,
                },
                event = self.swarm.select_next_some() => match event {
                    SwarmEvent::NewListenAddr { address,.. } => info!("Mesh listening on {:?}", address),
                    SwarmEvent::Behaviour(SovereignBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                        for (peer, addr) in list {
                            self.swarm.behaviour_mut().kademlia.add_address(&peer, addr);
                        }
                    },
                    _ => {}
                }
            }
        }
    }
}

/// Parses standard IPFS swarm.key files
fn load_swarm_key(path: &Path) -> anyhow::Result<PreSharedKey> {
    let content = std::fs::read_to_string(path)?;
    let mut lines = content.lines();
    if lines.next()!= Some("/key/swarm/psk/1.0.0/") {
        return Err(anyhow::anyhow!("Invalid swarm key header"));
    }
    if lines.next()!= Some("/base16/") {
        return Err(anyhow::anyhow!("Unsupported encoding"));
    }
    let key_hex = lines.next().ok_or(anyhow::anyhow!("Missing key data"))?;
    let bytes = hex::decode(key_hex.trim())?;
    let key_arr: [u8; 32] = bytes.try_into().map_err(|_| anyhow::anyhow!("Key must be exactly 32 bytes"))?;
    Ok(PreSharedKey::new(key_arr))
}
