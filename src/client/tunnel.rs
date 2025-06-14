//! Tunnel implementation for client

use crate::common::{error::Result, types::*};
use std::net::SocketAddr;

/// Tunnel manager for client
pub struct TunnelManager {
    tunnels: dashmap::DashMap<TunnelId, TunnelInfo>,
}

impl TunnelManager {
    /// Create a new tunnel manager
    pub fn new() -> Self {
        Self {
            tunnels: dashmap::DashMap::new(),
        }
    }
    
    /// Create a new tunnel
    pub async fn create_tunnel(
        &self,
        name: String,
        source: SocketAddr,
        bind: SocketAddr,
        router: SocketAddr,
        protocol: Protocol,
    ) -> Result<TunnelId> {
        let tunnel_id = TunnelId::new();
        let tunnel_info = TunnelInfo {
            id: tunnel_id.clone(),
            name,
            source,
            bind,
            router,
            protocol,
            status: TunnelStatus::Starting,
            active_connections: 0,
            bytes_transferred: 0,
            created_at: chrono::Utc::now(),
        };
        
        self.tunnels.insert(tunnel_id.clone(), tunnel_info);
        
        // TODO: Implement actual tunnel creation logic
        
        Ok(tunnel_id)
    }
    
    /// Get tunnel information
    pub fn get_tunnel(&self, tunnel_id: &TunnelId) -> Option<TunnelInfo> {
        self.tunnels.get(tunnel_id).map(|entry| entry.clone())
    }
    
    /// List all tunnels
    pub fn list_tunnels(&self) -> Vec<TunnelInfo> {
        self.tunnels.iter().map(|entry| entry.value().clone()).collect()
    }
    
    /// Remove a tunnel
    pub async fn remove_tunnel(&self, tunnel_id: &TunnelId) -> Result<()> {
        if let Some((_, mut tunnel_info)) = self.tunnels.remove(tunnel_id) {
            tunnel_info.status = TunnelStatus::Stopping;
            // TODO: Implement actual tunnel cleanup logic
        }
        Ok(())
    }
}

impl Default for TunnelManager {
    fn default() -> Self {
        Self::new()
    }
}