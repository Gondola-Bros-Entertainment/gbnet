//! # GB-Net
//!
//! A production-ready Rust game networking library providing reliable UDP
//! transport with bitpacked serialization.
//!
//! GB-Net operates at the transport level (similar to ENet, LiteNetLib, yojimbo)
//! using a synchronous, polling game-loop model.
//!
//! ## Features
//!
//! - **5 delivery modes**: Unreliable, UnreliableSequenced, ReliableUnordered,
//!   ReliableOrdered, ReliableSequenced
//! - **Bitpacked serialization** with `#[derive(NetworkSerialize)]` and `#[bits = N]`
//! - **Adaptive reliability**: Jacobson/Karels RTT, exponential backoff, fast retransmit
//! - **Binary congestion control** with message batching
//! - **Message fragmentation** and MTU discovery
//! - **Optional AES-256-GCM encryption** via the `encryption` feature
//! - **CRC32C** packet integrity checking
//!
//! ## Quick Start
//!
//! ```no_run
//! use gbnet::prelude::*;
//!
//! // Server
//! let addr = "127.0.0.1:7777".parse().unwrap();
//! let mut server = NetServer::bind(addr, NetworkConfig::default()).unwrap();
//!
//! loop {
//!     for event in server.update() {
//!         match event {
//!             ServerEvent::ClientConnected(addr) => println!("Connected: {}", addr),
//!             ServerEvent::Message { addr, channel, data } => {
//!                 server.send(addr, channel, &data).ok();
//!             }
//!             _ => {}
//!         }
//!     }
//! }
//! ```

extern crate self as gbnet;

pub mod channel;
pub mod client;
pub mod config;
pub mod congestion;
pub mod connection;
pub mod fragment;
pub mod packet;
pub mod reliability;
pub mod security;
pub mod serialize;
pub mod server;
pub mod simulator;
pub mod socket;
pub mod stats;
pub mod util;
pub mod wire;

#[cfg(test)]
mod tests;

pub use channel::{Channel, ChannelError};
pub use client::{ClientEvent, NetClient};
pub use config::{ChannelConfig, ConfigError, DeliveryMode, NetworkConfig, SimulationConfig};
pub use congestion::{BandwidthTracker, CongestionController, CongestionMode};
pub use connection::{Connection, ConnectionError, ConnectionState, DisconnectReason};
pub use fragment::{FragmentAssembler, FragmentError, FragmentHeader, MtuDiscovery};
pub use packet::{Packet, PacketHeader, PacketType};
pub use reliability::{ReliableEndpoint, SequenceBuffer};
pub use security::{crc32c, ConnectToken, ConnectionRateLimiter, TokenValidator};
pub use server::{NetServer, ServerEvent};
pub use simulator::NetworkSimulator;
pub use socket::{SocketError, UdpSocket};
pub use stats::{
    assess_connection_quality, ChannelStats, ConnectionQuality, NetworkStats, ReliabilityStats,
    SocketStats,
};
pub use util::{sequence_diff, sequence_greater_than};

pub use gbnet_macros::NetworkSerialize;

pub use serialize::bit_io::{BitBuffer, BitRead, BitWrite};
pub use serialize::{BitDeserialize, BitSerialize, ByteAlignedDeserialize, ByteAlignedSerialize};
pub use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Unified error type encompassing all GB-Net error variants.
#[derive(Debug)]
pub enum NetError {
    Socket(SocketError),
    Connection(ConnectionError),
    Channel(ChannelError),
    Security(security::TokenError),
    Io(std::io::Error),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::Socket(e) => write!(f, "Socket error: {}", e),
            NetError::Connection(e) => write!(f, "Connection error: {}", e),
            NetError::Channel(e) => write!(f, "Channel error: {}", e),
            NetError::Security(e) => write!(f, "Security error: {}", e),
            NetError::Io(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for NetError {}

impl From<SocketError> for NetError {
    fn from(err: SocketError) -> Self {
        NetError::Socket(err)
    }
}

impl From<ConnectionError> for NetError {
    fn from(err: ConnectionError) -> Self {
        NetError::Connection(err)
    }
}

impl From<ChannelError> for NetError {
    fn from(err: ChannelError) -> Self {
        NetError::Channel(err)
    }
}

impl From<std::io::Error> for NetError {
    fn from(err: std::io::Error) -> Self {
        NetError::Io(err)
    }
}

/// Prelude: import everything commonly needed.
pub mod prelude {
    pub use crate::{
        BitBuffer, BitDeserialize, BitRead, BitSerialize, BitWrite, ChannelConfig, ClientEvent,
        Connection, ConnectionError, ConnectionQuality, ConnectionState, DeliveryMode,
        DisconnectReason, NetClient, NetError, NetServer, NetworkConfig, NetworkSerialize,
        NetworkStats, ServerEvent, SocketAddr,
    };
}
