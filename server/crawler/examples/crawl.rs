use monero_crawler_lib::{CrawlBuilder, capability_checkers::CapabilitiesChecker};
use std::{fs::OpenOptions, io::Write};

use futures::StreamExt;

const ZMQ_PORTS: [u16; 2] = [18083, 18084];
const RPC_PORTS: [u16; 2] = [18081, 18089];

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut peers_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("peers.txt")
        .unwrap();
    let crawler = CrawlBuilder::default()
        .capabilities(vec![
            CapabilitiesChecker::Latency(100),
            CapabilitiesChecker::Rpc(RPC_PORTS.to_vec()),
            CapabilitiesChecker::Zmq(ZMQ_PORTS.to_vec()),
            CapabilitiesChecker::SpyNode(false, vec![]),
            CapabilitiesChecker::SeedNode(false),
        ])
        .build()
        .unwrap();
    let mut stream = crawler.discover_peers().await;
    while let Some((peer, rpc_port, zmq_port, ms)) = stream.next().await {
        println!("found a capable node");
        peers_file
            .write_fmt(format_args!("{peer:?}, {rpc_port}, {zmq_port}, {ms}\n"))
            .unwrap();
    }
}
