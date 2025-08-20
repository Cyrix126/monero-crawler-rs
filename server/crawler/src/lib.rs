use cuprate_p2p_core::transports::Tcp;
use cuprate_p2p_core::{PeerRequest, PeerResponse};
use cuprate_wire::{AdminRequestMessage, AdminResponseMessage, CoreSyncData};
use dashmap::DashSet;
use derive_builder::Builder;
use enclose::enc;
use futures::{FutureExt, future::BoxFuture, stream};
use futures::{Stream, StreamExt};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::{
    convert::Infallible,
    sync::{LazyLock, OnceLock},
    task::Poll,
    time::Duration,
};
use tokio::spawn;
use tokio::{
    sync::{Semaphore, mpsc},
    time::timeout,
};
use tokio_stream::wrappers::ReceiverStream;
use tower::{Service, ServiceExt, make::Shared, util::MapErr};
use zeromq::{Socket, ZmqError};

use cuprate_p2p_core::{
    BroadcastMessage, ClearNet, NetZoneAddress, Network, NetworkZone,
    client::{
        ConnectRequest, Connector, HandshakerBuilder, InternalPeerID,
        handshaker::builder::{DummyCoreSyncSvc, DummyProtocolRequestHandler},
    },
    services::{AddressBookRequest, AddressBookResponse},
};
use cuprate_wire::{BasicNodeData, common::PeerSupportFlags};

pub mod seeds;

type ConnectorService = Connector<
    ClearNet,
    Tcp,
    AddressBookService,
    DummyCoreSyncSvc,
    MapErr<Shared<DummyProtocolRequestHandler>, fn(Infallible) -> tower::BoxError>,
    fn(InternalPeerID<<ClearNet as NetworkZone>::Addr>) -> stream::Pending<BroadcastMessage>,
>;
static CONNECTOR: OnceLock<ConnectorService> = OnceLock::new();
static SCANNED_NODES: LazyLock<DashSet<SocketAddr>> = LazyLock::new(DashSet::new);
static PEERS_CHANNEL: OnceLock<
    mpsc::Sender<(SocketAddr, cuprate_p2p_core::client::Client<ClearNet>)>,
> = OnceLock::new();

#[derive(Builder)]
#[builder(pattern = "immutable")]
pub struct Crawl {
    #[builder(setter(into), default = "self.default_seeds()")]
    seeds: Vec<SocketAddr>,
    #[builder(default = "Duration::from_secs(5)")]
    timeout_duration: Duration,
    #[builder(default = "Arc::new(Semaphore::new(100))")]
    connections_limit: Arc<Semaphore>,
    #[builder(default)]
    chainnet: Network,
    #[builder(default)]
    capabilities: Vec<CapabilitiesChecker>,
    #[cfg(feature = "rpc")]
    #[builder(default)]
    client: reqwest::Client,
}

impl CrawlBuilder {
    fn default_seeds(&self) -> Vec<SocketAddr> {
        seeds::SEED_NODES.to_vec()
    }
}

/// array of u16 represent ports on which at least one is able to support the service
#[derive(Clone)]
pub enum CapabilitiesChecker {
    Latency(u32),
    // the list of ports to scan for rpc. If the list is empty, the only port used to check if rpc is enabled on peer is the port advertised through p2p
    #[cfg(feature = "rpc")]
    Rpc(Vec<u16>),
    #[cfg(feature = "zmq")]
    Zmq(Vec<u16>),
    ChainSynced(Arc<Mutex<u64>>),
    // true: only spy nodes
    // false, exclude spy nodes
    // If a vec is empty, detect manually if the peer is a spy node.
    // If
    SpyNode(bool, Vec<SocketAddr>),
}

impl Crawl {
    /// Will return only peers that checks capabilities
    /// The return is peer address, rpc port, zmq port, latency
    /// If the zmq or rpc or latency capability is not used, value will be set to 0
    pub async fn discover_peers(&self) -> impl Stream<Item = (SocketAddr, u16, u16, u32)> {
        let (capable_peers_tx, capable_peers_rx) = mpsc::channel(508);
        let handshaker = HandshakerBuilder::<ClearNet, Tcp>::new(
            BasicNodeData {
                my_port: 0,
                network_id: self.chainnet.network_id(),
                peer_id: rand::random(),
                support_flags: PeerSupportFlags::FLUFFY_BLOCKS,
                rpc_port: 0,
                rpc_credits_per_hash: 0,
            },
            (),
        )
        .with_address_book(AddressBookService {
            timeout: self.timeout_duration,
            connections_limit: self.connections_limit.clone(),
        })
        .build();

        let capabilities = self.capabilities.clone();
        let client = &self.client;
        let timeout_duration = self.timeout_duration;
        let seeds = &self.seeds;
        let connections_limit = &self.connections_limit;
        spawn(
            enc!((capable_peers_tx, capabilities, timeout_duration, client, seeds, connections_limit) async move {
            let connector = Connector::new(handshaker);

            let _ = CONNECTOR.get_or_init(|| connector.clone());

            // the buffer is max the number of peers to store in memory without being yield.
            let (peers_tx, mut peers_rx) = mpsc::channel(508);

            // set only if it is not already by not checking the error
            // This will happen if the discover_peers is ran multiple times
            let _ = PEERS_CHANNEL.set(peers_tx);

            seeds.iter().for_each(|peer| {
                tokio::spawn(request_book_node(
                    *peer,
                    timeout_duration,
                    connections_limit.clone(),
                ));
            });
            while let Some((socket, mut peer)) = peers_rx.recv().await {
                tokio::spawn(
                    enc!((capable_peers_tx, capabilities, timeout_duration, client) async move {
                        let mut rpc_port = 0;
                        let mut zmq_port = 0;
                        let mut latency = 0;
                        for capability in capabilities.iter() {
                            match capability {
                                CapabilitiesChecker::Latency(max) => if let Some(ms) = is_latency_capable(socket, timeout_duration, *max).await {
                                                                latency = ms;
                                } else {
                                    return;
                                }
                                #[cfg(feature = "rpc")]
                                CapabilitiesChecker::Rpc(ports) => {
                                    if let Some(port) = is_rpc_capable(socket, peer.info.basic_node_data.rpc_port, ports, timeout_duration, &client).await {
                                        rpc_port = port;
                                } else {
                                    return;

                                }}
                                #[cfg(feature = "zmq")]
                                CapabilitiesChecker::Zmq(ports) => if let Some(port) = is_zmq_capable(socket, ports, timeout_duration).await {
                                    zmq_port = port;
                                } else {
                                    return;
                                }
                                CapabilitiesChecker::ChainSynced(chain_height) => {
                                    if !is_blockchain_synced(&peer.info.core_sync_data, chain_height.clone()).await {
                                    return;
                                }
                                    }
                                CapabilitiesChecker::SpyNode(keep_only_spynode, blacklist) => if !is_spynode(&mut peer, socket, blacklist).await.is_ok_and(|r|r) && *keep_only_spynode {
                                        return;
                                }
                            }
                        }
                            capable_peers_tx.send((socket, rpc_port, zmq_port, latency)).await.unwrap();
                    }),
                );
            }

                }),
        );
        ReceiverStream::new(capable_peers_rx)
    }
}

async fn is_spynode(
    peer: &mut cuprate_p2p_core::client::Client<ClearNet>,
    socket: SocketAddr,
    blacklist: &[SocketAddr],
) -> Result<bool, tower::BoxError> {
    // if a list is given, check it only, do not check the peer manually
    if !blacklist.is_empty() {
        if blacklist.contains(&socket) {
            return Ok(true);
        } else {
            return Ok(false);
        }
    }
    let PeerResponse::Admin(AdminResponseMessage::Ping(ping)) = peer
        .ready()
        .await?
        .call(PeerRequest::Admin(AdminRequestMessage::Ping))
        .await?
    else {
        return Ok(false);
    };

    let PeerResponse::Admin(AdminResponseMessage::Ping(ping_2)) = peer
        .ready()
        .await?
        .call(PeerRequest::Admin(AdminRequestMessage::Ping))
        .await?
    else {
        return Ok(false);
    };

    let PeerResponse::Admin(AdminResponseMessage::Ping(ping_3)) = peer
        .ready()
        .await?
        .call(PeerRequest::Admin(AdminRequestMessage::Ping))
        .await?
    else {
        return Ok(false);
    };

    Ok(peer.info.basic_node_data.peer_id != ping.peer_id
        || ping.peer_id != ping_2.peer_id
        || ping_2.peer_id != ping_3.peer_id)
}

/// We don't know the current height of the network, so it needs to be given from an outside source.
/// Since the current height of the network can change, it is wrapped into an arc mutex
async fn is_blockchain_synced(
    core_sync_data: &Arc<Mutex<CoreSyncData>>,
    height: Arc<Mutex<u64>>,
) -> bool {
    core_sync_data.lock().unwrap().current_height >= *height.lock().unwrap()
}
/// Will return opened ports, will stop scanning when one is found and still in the buffer.
async fn check_ports(
    peer: SocketAddr,
    ports: &[u16],
    timeout_duration: Duration,
) -> impl Stream<Item = u16> {
    let (open_ports_tx, open_ports_rx) = mpsc::channel(1);
    for port in ports {
        tokio::spawn(enc!((open_ports_tx, port) async move {
            let mut socket_addr_peer = peer;
            socket_addr_peer.set_port(port);
            if timeout(
                timeout_duration,
                tokio::net::TcpStream::connect(socket_addr_peer),
            )
            .await
            .is_ok_and(|r| r.is_ok())
            {
                let _ = open_ports_tx.send(port).await;
            }
        }));
    }
    ReceiverStream::new(open_ports_rx)
}
/// The port is checked before sending a request to verify that the service is responding
/// Even if multiple ports are opened
/// port is advertized if monerod was launched with --public-node argument. A rpc enabled node can exist without this argument.
/// returns the port in either case
async fn is_rpc_capable(
    addr: SocketAddr,
    port_advertized: u16,
    ports: &[u16],
    timeout_duration: Duration,
    client: &reqwest::Client,
) -> Option<u16> {
    let mut socket_addr_peer = addr;
    // port advertized is checked first
    if port_advertized != 0 {
        // we check the rpc service without checking if the port open, as it should be opened in most case if it was advertised by the peer.
        // If the port is not opened, the check will just fail with the timeout
        socket_addr_peer.set_port(port_advertized);
        if rpc_check(socket_addr_peer, timeout_duration, client).await {
            return Some(port_advertized);
        }
    }
    // if the advertized port was not functional or there was no advertized port, check the given ports
    let mut ports_stream = check_ports(addr, ports, timeout_duration).await;
    while let Some(port) = ports_stream.next().await {
        let mut socket_addr_peer = addr;
        socket_addr_peer.set_port(port);
        if rpc_check(socket_addr_peer, timeout_duration, client).await {
            return Some(port);
        }
    }
    None
}
/// P2P messages does not contains a publicized zmq port.
/// The port is checked before sending a request to verify that the service is responding
async fn is_zmq_capable(
    addr: SocketAddr,
    ports: &[u16],
    timeout_duration: Duration,
) -> Option<u16> {
    let mut ports_stream = check_ports(addr, ports, timeout_duration).await;
    while let Some(port) = ports_stream.next().await {
        let mut socket_addr_peer = addr;
        socket_addr_peer.set_port(port);
        if zmq_check(socket_addr_peer, timeout_duration).await {
            return Some(port);
        }
    }
    None
}
#[cfg(feature = "rpc")]
async fn rpc_check(
    socket_peer: SocketAddr,
    timeout_duration: Duration,
    client: &reqwest::Client,
) -> bool {
    let request_rpc = client
        .post(
            "http://".to_string()
                + &socket_peer.ip().to_string()
                + ":"
                + &socket_peer.port().to_string()
                + "/json_rpc",
        )
        .body(r#"{"jsonrpc":"2.0","id":"0","method":"get_info"}"#);
    let resp = timeout(timeout_duration, request_rpc.send()).await;
    if resp
        .as_ref()
        .ok()
        .is_some_and(|e| e.as_ref().ok().is_some_and(|r| r.status().is_success()))
    {
        return true;
    }
    false
}
async fn zmq_check(socket_peer: SocketAddr, timeout_duration: Duration) -> bool {
    let mut req_socket = zeromq::ReqSocket::new();

    if let Ok(res) = timeout(
        timeout_duration,
        req_socket.connect(&format!(
            "tcp://{}:{}",
            socket_peer.ip(),
            socket_peer.port()
        )),
    )
    .await
    {
        // current rust native zmq lib doesn't not connect well, but the error message for nodes with zmq-pub enabled is different than the ones without
        if let Err(e) = res
            && let ZmqError::Other(msg) = e
            && msg == "Provided sockets combination is not compatible"
        {
            return true;
        }
    }
    false
}

async fn is_latency_capable(
    socket: SocketAddr,
    timeout_duration: Duration,
    max: u32,
) -> Option<u32> {
    let now = Instant::now();
    if (timeout(timeout_duration, tokio::net::TcpStream::connect(socket)).await).is_ok() {
        let ms = now.elapsed().as_millis() as u32;
        if ms <= max {
            return Some(ms);
        }
    }
    None
}

async fn request_book_node(
    addr: SocketAddr,
    timeout_duration: Duration,
    connection_limit: Arc<Semaphore>,
) -> Result<(), tower::BoxError> {
    let _guard = connection_limit.acquire().await.unwrap();
    let mut connector = CONNECTOR.get().unwrap().clone();
    let peer = timeout(
        timeout_duration,
        connector
            .ready()
            .await?
            .call(ConnectRequest { addr, permit: None }),
    )
    .await??;
    PEERS_CHANNEL.get().unwrap().send((addr, peer)).await?;
    Ok(())
}

#[derive(Clone)]
struct AddressBookService {
    timeout: Duration,
    connections_limit: Arc<Semaphore>,
}

impl Service<AddressBookRequest<ClearNet>> for AddressBookService {
    type Error = tower::BoxError;
    type Response = AddressBookResponse<ClearNet>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: AddressBookRequest<ClearNet>) -> Self::Future {
        let duration = self.timeout;
        let connections_limit = &self.connections_limit;
        enc!((duration, connections_limit) async move {
            match req {
                AddressBookRequest::IncomingPeerList(_, peers) => {
                    for mut peer in peers {
                        peer.adr.make_canonical();
                        if SCANNED_NODES.insert(peer.adr) {
                            tokio::spawn(enc!((duration, connections_limit) async move {
                                if request_book_node(peer.adr, duration, connections_limit).await.is_err() {
                                    SCANNED_NODES.remove(&peer.adr);
                                }
                            }));
                        }
                    }

                    Ok(AddressBookResponse::Ok)
                }
                AddressBookRequest::NewConnection { .. } => Ok(AddressBookResponse::Ok),
                AddressBookRequest::GetWhitePeers(_) => Ok(AddressBookResponse::Peers(vec![])),
                _ => Err("no peers".into()),
            }
        })
        .boxed()
    }
}
