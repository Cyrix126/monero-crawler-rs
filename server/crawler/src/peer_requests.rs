use std::net::SocketAddr;

use cuprate_p2p_core::client::HandshakeError;
use cuprate_wire::{
    AdminRequestMessage, AdminResponseMessage, BasicNodeData, CoreSyncData, LevinCommand, Message,
    MoneroWireCodec,
    admin::{HandshakeRequest, HandshakeResponse, PingResponse},
};
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_util::codec::{FramedRead, FramedWrite};

use crate::{PeerSink, PeerStream};

pub async fn send_handshake(
    our_core_sync_data: CoreSyncData,
    our_basic_node_data: BasicNodeData,
    peer_sink: &mut PeerSink,
) -> Result<(), HandshakeError> {
    let req = HandshakeRequest {
        node_data: our_basic_node_data,
        payload_data: our_core_sync_data,
    };
    peer_sink
        .send(Message::Request(AdminRequestMessage::Handshake(req)).into())
        .await?;
    Ok(())
}

/// We ignore all response that are not handshake response.
pub async fn receive_response_handshake(peer_stream: &mut PeerStream) -> Option<HandshakeResponse> {
    while let Some(message) = peer_stream.next().await {
        if let Ok(message) = message
            && let Message::Response(res_message) = message
            && res_message.command() == LevinCommand::Handshake
            && let AdminResponseMessage::Handshake(res) = res_message
        {
            return Some(res);
        }
    }

    None
}

pub async fn ping(sink: &mut PeerSink, stream: &mut PeerStream) -> Option<PingResponse> {
    sink.send(Message::Request(AdminRequestMessage::Ping).into())
        .await
        .ok()?;
    if let Some(res) = stream.next().await
        && let Message::Response(AdminResponseMessage::Ping(ping)) = res.ok()?
    {
        return Some(ping);
    }
    None
}
pub async fn fetch_peer(
    addr: &SocketAddr,
    core_sync_data: CoreSyncData,
    basic_node_data: BasicNodeData,
) -> Option<HandshakeResponse> {
    let (mut stream, mut sink) = connect_to_peer(addr).await?;
    send_handshake(core_sync_data, basic_node_data, &mut sink)
        .await
        .ok()?;

    let resp = receive_response_handshake(&mut stream).await?;
    Some(resp)
}

pub async fn connect_to_peer(addr: &SocketAddr) -> Option<(PeerStream, PeerSink)> {
    let (read, write) = TcpStream::connect(addr).await.ok()?.into_split();
    Some((
        FramedRead::new(read, MoneroWireCodec::default()),
        FramedWrite::new(write, MoneroWireCodec::default()),
    ))
}
