//! Common types used throughout the application

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;

/// Tunnel identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TunnelId(pub Uuid);

impl TunnelId {
    /// Create a new tunnel ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TunnelId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TunnelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Connection identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionId(pub Uuid);

impl ConnectionId {
    /// Create a new connection ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Tunnel status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TunnelStatus {
    /// Tunnel is starting up
    Starting,
    /// Tunnel is active and ready
    Active,
    /// Tunnel is stopping
    Stopping,
    /// Tunnel has stopped
    Stopped,
    /// Tunnel is in error state
    Error(String),
}

impl std::fmt::Display for TunnelStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TunnelStatus::Starting => write!(f, "Starting"),
            TunnelStatus::Active => write!(f, "Active"),
            TunnelStatus::Stopping => write!(f, "Stopping"),
            TunnelStatus::Stopped => write!(f, "Stopped"),
            TunnelStatus::Error(msg) => write!(f, "Error: {}", msg),
        }
    }
}

/// Connection status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    /// Connection is being established
    Connecting,
    /// Connection is active
    Active,
    /// Connection is being closed
    Closing,
    /// Connection is closed
    Closed,
    /// Connection is in error state
    Error(String),
}

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionStatus::Connecting => write!(f, "Connecting"),
            ConnectionStatus::Active => write!(f, "Active"),
            ConnectionStatus::Closing => write!(f, "Closing"),
            ConnectionStatus::Closed => write!(f, "Closed"),
            ConnectionStatus::Error(msg) => write!(f, "Error: {}", msg),
        }
    }
}

/// Protocol type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    Tcp,
    Udp,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Tcp => write!(f, "TCP"),
            Protocol::Udp => write!(f, "UDP"),
        }
    }
}

impl std::str::FromStr for Protocol {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tcp" => Ok(Protocol::Tcp),
            "udp" => Ok(Protocol::Udp),
            _ => Err(format!("Invalid protocol: {}", s)),
        }
    }
}

/// Tunnel information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelInfo {
    /// Unique tunnel identifier
    pub id: TunnelId,
    /// Human-readable tunnel name
    pub name: String,
    /// Source service address (on router side)
    pub source: SocketAddr,
    /// Local bind address
    pub bind: SocketAddr,
    /// Router address
    pub router: SocketAddr,
    /// Protocol type
    pub protocol: Protocol,
    /// Current status
    pub status: TunnelStatus,
    /// Number of active connections
    pub active_connections: u32,
    /// Total bytes transferred
    pub bytes_transferred: u64,
    /// Tunnel creation time
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Unique connection identifier
    pub id: ConnectionId,
    /// Associated tunnel ID
    pub tunnel_id: TunnelId,
    /// Client address
    pub client_addr: SocketAddr,
    /// Target address
    pub target_addr: SocketAddr,
    /// Current status
    pub status: ConnectionStatus,
    /// Bytes transferred in this connection
    pub bytes_transferred: u64,
    /// Connection establishment time
    pub connected_at: chrono::DateTime<chrono::Utc>,
}