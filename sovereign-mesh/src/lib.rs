use futures::StreamExt;
use libp2p::{
    gossipsub, kad, mdns, noise,
    swarm::{NetworkBehaviour, SwarmEvent, Config},
    tcp, yamux, Multiaddr, PeerId, Swarm, Transport,
};
use std::path::Path;
use tokio::sync::{mpsc, oneshot};
use log::info;

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

        // Build transport
        let tcp_config = tcp::Config::default().nodelay(true);
        let noise_config = noise::Config::new(&id_keys).expect("Noise key gen failed");

        let transport = tcp::tokio::Transport::new(tcp_config)
            .upgrade(libp2p::core::upgrade::Version::V1)
            .authenticate(noise_config)
            .multiplex(yamux::Config::default())
            .boxed();

        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(id_keys.clone()),
            gossipsub::Config::default(),
        ).map_err(anyhow::Error::msg)?;

        let kademlia = kad::Behaviour::new(peer_id, kad::store::MemoryStore::new(peer_id));
        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?;
        let behaviour = SovereignBehaviour { gossipsub, kademlia, mdns };

        // Create Swarm with default config
        let mut swarm = Swarm::new(transport, behaviour, peer_id, Config::with_tokio_executor());
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
