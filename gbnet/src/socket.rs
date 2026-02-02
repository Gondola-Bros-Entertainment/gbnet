//! Platform-agnostic non-blocking UDP socket wrapper with statistics tracking.
use std::io::{Error as IoError, ErrorKind};
use std::net::{SocketAddr, UdpSocket as StdUdpSocket};
use std::time::{Duration, Instant};

use crate::stats::SocketStats;

/// Maximum size of a single UDP datagram.
const MAX_UDP_PACKET_SIZE: usize = 65536;

/// Errors that can occur during socket operations.
#[derive(Debug)]
pub enum SocketError {
    Io(IoError),
    InvalidAddress,
    SocketClosed,
    WouldBlock,
    Other(String),
}

impl std::fmt::Display for SocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SocketError::Io(e) => write!(f, "IO error: {}", e),
            SocketError::InvalidAddress => write!(f, "Invalid address"),
            SocketError::SocketClosed => write!(f, "Socket closed"),
            SocketError::WouldBlock => write!(f, "Operation would block"),
            SocketError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for SocketError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SocketError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<IoError> for SocketError {
    fn from(err: IoError) -> Self {
        match err.kind() {
            ErrorKind::WouldBlock => SocketError::WouldBlock,
            _ => SocketError::Io(err),
        }
    }
}

/// Non-blocking UDP socket with per-socket statistics.
pub struct UdpSocket {
    socket: StdUdpSocket,
    recv_buffer: Vec<u8>,
    stats: SocketStats,
}

impl UdpSocket {
    /// Creates a new UDP socket bound to the specified address
    pub fn bind(addr: SocketAddr) -> Result<Self, SocketError> {
        let socket = StdUdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;

        Ok(Self {
            socket,
            recv_buffer: vec![0u8; MAX_UDP_PACKET_SIZE],
            stats: SocketStats::default(),
        })
    }

    /// Returns the local address this socket is bound to
    pub fn local_addr(&self) -> Result<SocketAddr, SocketError> {
        Ok(self.socket.local_addr()?)
    }

    /// Sends data to a specific address
    pub fn send_to(&mut self, data: &[u8], addr: SocketAddr) -> Result<usize, SocketError> {
        let sent = self.socket.send_to(data, addr)?;
        self.stats.bytes_sent += sent as u64;
        self.stats.packets_sent += 1;
        self.stats.last_send_time = Some(Instant::now());
        Ok(sent)
    }

    /// Receives data from any address (returns data slice and sender address)
    pub fn recv_from(&mut self) -> Result<(&[u8], SocketAddr), SocketError> {
        match self.socket.recv_from(&mut self.recv_buffer) {
            Ok((len, addr)) => {
                self.stats.bytes_received += len as u64;
                self.stats.packets_received += 1;
                self.stats.last_receive_time = Some(Instant::now());
                Ok((&self.recv_buffer[..len], addr))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Sets the read timeout for the socket
    pub fn set_read_timeout(&self, dur: Option<Duration>) -> Result<(), SocketError> {
        self.socket.set_read_timeout(dur)?;
        Ok(())
    }

    /// Sets the write timeout for the socket
    pub fn set_write_timeout(&self, dur: Option<Duration>) -> Result<(), SocketError> {
        self.socket.set_write_timeout(dur)?;
        Ok(())
    }

    /// Returns socket statistics
    pub fn stats(&self) -> &SocketStats {
        &self.stats
    }

    /// Resets socket statistics
    pub fn reset_stats(&mut self) {
        self.stats = SocketStats::default();
    }
}
