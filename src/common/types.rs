// アプリケーション全体で使用される共通型定義

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TunnelId(pub Uuid);

impl TunnelId {
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionId(pub Uuid);

impl ConnectionId {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TunnelStatus {
    Starting,
    Active,
    Stopping,
    Stopped,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connecting,
    Active,
    Closing,
    Closed,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelInfo {
    pub id: TunnelId,
    pub name: String,
    // Router側のサービスアドレス
    pub source: SocketAddr,
    // Clientのローカルバインドアドレス
    pub bind: SocketAddr,
    pub router: SocketAddr,
    pub protocol: Protocol,
    pub status: TunnelStatus,
    pub active_connections: u32,
    pub bytes_transferred: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub id: ConnectionId,
    pub tunnel_id: TunnelId,
    pub client_addr: SocketAddr,
    pub target_addr: SocketAddr,
    pub status: ConnectionStatus,
    pub bytes_transferred: u64,
    pub connected_at: chrono::DateTime<chrono::Utc>,
}