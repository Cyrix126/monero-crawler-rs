use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use cuprate_wire::{CoreSyncData, admin::HandshakeResponse};
use enclose::enc;
use futures::Stream;
use tokio::{sync::mpsc::channel, time::timeout};
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use zeromq::{Socket, ZmqError};

use crate::peer_requests::{connect_to_peer, ping};

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
    SpyNode(bool, Vec<SocketAddr>),
    // Return only seed nodes or exclude them
    SeedNode(bool),
}
pub async fn is_spynode(
    socket: &SocketAddr,
    peer_data: &HandshakeResponse,
    blacklist: &[SocketAddr],
) -> Option<bool> {
    // if a list is given, check it only, do not check the peer manually
    if !blacklist.is_empty() {
        if blacklist.contains(socket) {
            return Some(true);
        } else {
            return Some(false);
        }
    }
    // note: don't we want to reuse the stream and sink opened for the handshake ?
    let (mut stream, mut sink) = connect_to_peer(socket).await?;
    let ping1 = ping(&mut sink, &mut stream).await?;
    let ping2 = ping(&mut sink, &mut stream).await?;
    let ping3 = ping(&mut sink, &mut stream).await?;

    Some(
        peer_data.node_data.peer_id != ping1.peer_id
            || ping1.peer_id != ping2.peer_id
            || ping2.peer_id != ping3.peer_id,
    )
}

/// We don't know the current height of the network, so it needs to be given from an outside source.
/// Since the current height of the network can change, it is wrapped into an arc mutex
pub async fn is_blockchain_synced(core_sync_data: &CoreSyncData, height: Arc<Mutex<u64>>) -> bool {
    core_sync_data.current_height >= *height.lock().unwrap()
}
/// Will return opened ports, will stop scanning when one is found and still in the buffer.
pub async fn check_ports(
    peer: SocketAddr,
    ports: &[u16],
    timeout_duration: Duration,
) -> impl Stream<Item = u16> {
    let (open_ports_tx, open_ports_rx) = channel(1);
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
pub async fn is_rpc_capable(
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
pub async fn is_zmq_capable(
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
pub async fn rpc_check(
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
pub async fn zmq_check(socket_peer: SocketAddr, timeout_duration: Duration) -> bool {
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

pub async fn is_latency_capable(
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
