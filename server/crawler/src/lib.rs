use cuprate_wire::admin::HandshakeResponse;
use cuprate_wire::{CoreSyncData, MoneroWireCodec, NetworkAddress};
use dashmap::DashSet;
use derive_builder::Builder;
use enclose::enc;
use futures::Stream;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::spawn;
use tokio::sync::mpsc::Sender;
use tokio::{
    sync::{Semaphore, mpsc},
    time::timeout,
};
use tokio_stream::wrappers::ReceiverStream;

use cuprate_p2p_core::Network;
use cuprate_wire::{BasicNodeData, common::PeerSupportFlags};

use crate::capability_checkers::{
    CapabilitiesChecker, is_blockchain_synced, is_latency_capable, is_rpc_capable, is_spynode,
    is_zmq_capable,
};
use crate::peer_requests::fetch_peer;
use crate::seeds::SEED_NODES;

pub mod capability_checkers;
mod peer_requests;
pub mod seeds;

use tokio_util::codec::{FramedRead, FramedWrite};
type PeerStream = FramedRead<OwnedReadHalf, MoneroWireCodec>;
type PeerSink = FramedWrite<OwnedWriteHalf, MoneroWireCodec>;

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

impl Crawl {
    /// Will return only peers that checks capabilities
    /// The return is peer address, rpc port, zmq port, latency
    /// If the zmq or rpc or latency capability is not used, value will be set to 0
    pub async fn discover_peers(&self) -> impl Stream<Item = (SocketAddr, u16, u16, u32)> {
        let (capable_peers_tx, capable_peers_rx) = mpsc::channel(508);

        let capabilities = &self.capabilities;
        let client = &self.client;
        let timeout_duration = self.timeout_duration;
        let connections_limit = &self.connections_limit;
        let chainnet = self.chainnet;
        let addrs = self.seeds.clone();
        spawn(
            enc!((capable_peers_tx, capabilities, timeout_duration, client, connections_limit) async move {
                // the buffer is max the number of peers to store in memory without being yield.
            let (peers_tx, mut peers_rx) = mpsc::channel::<(SocketAddr, HandshakeResponse)>(508);
            let peers_tx_arc = Arc::new(peers_tx);
            let scanned_nodes = Arc::new(DashSet::new());
                let basic_node_date = BasicNodeData {
                    my_port: 0,
                    network_id: chainnet.network_id(),
                    peer_id: rand::random(),
                    support_flags: PeerSupportFlags::FLUFFY_BLOCKS,
                    rpc_port: 0,
                    rpc_credits_per_hash: 0,
                };
                let core_sync_data = CoreSyncData::new(u128::default(), u64::default() , u32::default(), [0; 32], u8::default());





                            crawl_peers(addrs, timeout_duration, connections_limit,  peers_tx_arc, scanned_nodes, core_sync_data, basic_node_date);
                while let Some((socket, peer)) = peers_rx.recv().await {
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
                                    CapabilitiesChecker::SeedNode(keep_seed) => if SEED_NODES.contains(&socket) {
                                        if !keep_seed {
                                            return;
                                        }
                                    } else if *keep_seed {
                                        return;
                                    }
                                    #[cfg(feature = "rpc")]
                                    CapabilitiesChecker::Rpc(ports) => {
                                        if let Some(port) = is_rpc_capable(socket, peer.node_data.rpc_port, ports, timeout_duration, &client).await {
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
                                        if !is_blockchain_synced(&peer.payload_data, chain_height.clone()).await {
                                        return;
                                    }
                                        }
                                    CapabilitiesChecker::SpyNode(keep_only_spynode, blacklist) => if !is_spynode(&socket, &peer,  blacklist).await.is_some_and(|r|r)  && *keep_only_spynode {
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

fn crawl_peers(
    addrs: Vec<SocketAddr>,
    timeout_duration: Duration,
    connection_limit: Arc<Semaphore>,
    peers_channel: Arc<Sender<(SocketAddr, HandshakeResponse)>>,
    scanned_nodes: Arc<DashSet<SocketAddr>>,
    core_sync_data: CoreSyncData,
    basic_node_data: BasicNodeData,
) {
    for addr in addrs {
        if !scanned_nodes.insert(addr) {
            // do we need to remove from a dashset ? does the dashset insert duplicates ?
            scanned_nodes.remove(&addr);
            return;
        }
        // for each addr, fetch the peer data. If successful, add them to the peer channel
        // there is a clone of value here
        tokio::spawn(
            enc!((connection_limit, basic_node_data, core_sync_data, peers_channel, scanned_nodes) async move {
            let guard = connection_limit.acquire().await.unwrap();
                    let peer = timeout(
                        timeout_duration,
                        fetch_peer(&addr, core_sync_data.clone(), basic_node_data.clone()),
                    )
                    .await.ok()??;
                    peers_channel.send((addr, peer.clone())).await.unwrap();
                    // this might cause some computing
                    let addrs = peer
                        .local_peerlist_new
                        .iter()
                        .filter_map(|p| {
                            if let NetworkAddress::Clear(socket) = p.adr {
                                Some(socket)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<SocketAddr>>();
                        {
                            drop(guard);
                        crawl_peers(
                            addrs,
                            timeout_duration,
                            connection_limit,
                            peers_channel,
                            scanned_nodes,
                            core_sync_data,
                            basic_node_data,
                        );
                        }
            None::<()>
                }),
        );
    }
}
