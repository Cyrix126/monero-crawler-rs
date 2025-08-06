use std::net::{Ipv4Addr, SocketAddr};

/// Seeds nodes in socket address type compile time constants
pub const SEED_NODES: [SocketAddr; 7] = [
    std::net::SocketAddr::V4(std::net::SocketAddrV4::new(
        Ipv4Addr::new(176, 9, 0, 187),
        18080,
    )),
    std::net::SocketAddr::V4(std::net::SocketAddrV4::new(
        Ipv4Addr::new(88, 198, 163, 90),
        18080,
    )),
    std::net::SocketAddr::V4(std::net::SocketAddrV4::new(
        Ipv4Addr::new(66, 85, 74, 134),
        18080,
    )),
    std::net::SocketAddr::V4(std::net::SocketAddrV4::new(
        Ipv4Addr::new(51, 79, 173, 165),
        18080,
    )),
    std::net::SocketAddr::V4(std::net::SocketAddrV4::new(
        Ipv4Addr::new(192, 99, 8, 110),
        18080,
    )),
    std::net::SocketAddr::V4(std::net::SocketAddrV4::new(
        Ipv4Addr::new(37, 187, 74, 171),
        18080,
    )),
    std::net::SocketAddr::V4(std::net::SocketAddrV4::new(
        Ipv4Addr::new(88, 99, 195, 15),
        18080,
    )),
];
