// Conduitのクライアントモジュール
//
// Routerに接続してトンネルを管理するクライアント側機能を実装

pub mod tunnel;
pub mod connection;
pub mod config;

pub use config::{ClientConfig, ClientInfo, RouterConfig, ConnectionSettings, TunnelConfig};
pub use connection::{ConnectionManager, ConnectionConfig, ConnectionEvent, ConnectionStats};
pub use tunnel::TunnelManager;

use crate::common::error::Result;
use crate::security::{TlsClientConfig, AuthManager};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, error, instrument};

pub struct Client {
    config: ClientConfig,
    tunnel_manager: TunnelManager,
    connection_manager: Option<ConnectionManager>,
}

impl Client {
    pub fn new(config: ClientConfig) -> Result<Self> {
        let tls_config = TlsClientConfig::new(&config.security.tls)
            .map_err(|e| crate::common::error::Error::Security(format!("TLS config error: {}", e)))?;
        
        // 簡略化: デフォルト値でAuthManagerを作成
        let key_manager = crate::security::KeyManager::new(
            "./keys",
            crate::security::KeyRotationConfig::default()
        ).expect("Failed to create KeyManager");
        let session_timeout = std::time::Duration::from_secs(3600);
        let token_duration = std::time::Duration::from_secs(1800);
        let auth_manager = Arc::new(AuthManager::new(key_manager, session_timeout, token_duration));
        
        let tunnel_manager = TunnelManager::new(tls_config, auth_manager);
        
        Ok(Self {
            config,
            tunnel_manager,
            connection_manager: None,
        })
    }
    
    #[instrument(skip(self))]
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting Conduit client: {}", self.config.client.name);
        
        let connection_config = self.config.to_connection_config();
        let tls_config = TlsClientConfig::new(&self.config.security.tls)
            .map_err(|e| crate::common::error::Error::Security(format!("TLS config error: {}", e)))?;
        let key_manager = crate::security::KeyManager::new(
            "./keys",
            crate::security::KeyRotationConfig::default()
        ).expect("Failed to create KeyManager");
        let session_timeout = std::time::Duration::from_secs(3600);
        let token_duration = std::time::Duration::from_secs(1800);
        let auth_manager = Arc::new(AuthManager::new(key_manager, session_timeout, token_duration));
        
        let mut connection_manager = ConnectionManager::new(
            connection_config,
            tls_config,
            auth_manager,
        );
        
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        connection_manager.set_event_channel(event_tx);
        
        connection_manager.start().await?;
        
        self.tunnel_manager.set_connection_manager(connection_manager).await;
        
        for tunnel_config in &self.config.tunnels {
            if tunnel_config.enabled {
                if let Err(e) = self.start_tunnel(tunnel_config).await {
                    error!("Failed to start tunnel {}: {}", tunnel_config.name, e);
                }
            }
        }
        
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                match event {
                    ConnectionEvent::Connected => {
                        info!("Connected to router");
                    }
                    ConnectionEvent::Disconnected => {
                        info!("Disconnected from router");
                    }
                    ConnectionEvent::Reconnecting => {
                        info!("Reconnecting to router");
                    }
                    ConnectionEvent::Authenticated => {
                        info!("Authenticated with router");
                    }
                    ConnectionEvent::Error(err) => {
                        error!("Connection error: {}", err);
                    }
                    ConnectionEvent::HeartbeatSent => {
                        info!("Heartbeat sent");
                    }
                    ConnectionEvent::HeartbeatReceived => {
                        info!("Heartbeat received");
                    }
                }
            }
        });
        
        info!("Client started successfully");
        Ok(())
    }
    
    async fn start_tunnel(&self, tunnel_config: &TunnelConfig) -> Result<()> {
        let protocol = match tunnel_config.protocol.as_str() {
            "tcp" => crate::common::types::Protocol::Tcp,
            "udp" => crate::common::types::Protocol::Udp,
            _ => return Err(crate::common::error::Error::Config(
                format!("Unsupported protocol: {}", tunnel_config.protocol)
            )),
        };
        
        let router_addr = format!("{}:{}", self.config.router.host, self.config.router.port)
            .parse()
            .map_err(|e| crate::common::error::Error::Config(
                format!("Invalid router address: {}", e)
            ))?;
        
        self.tunnel_manager.create_tunnel(
            tunnel_config.name.clone(),
            tunnel_config.source,
            tunnel_config.bind,
            router_addr,
            protocol,
        ).await?;
        
        info!("Tunnel started: {}", tunnel_config.name);
        Ok(())
    }
    
    #[instrument(skip(self))]
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping Conduit client");
        
        let tunnels = self.tunnel_manager.list_tunnels();
        for tunnel in tunnels {
            if let Err(e) = self.tunnel_manager.remove_tunnel(&tunnel.id).await {
                error!("Failed to stop tunnel {}: {}", tunnel.name, e);
            }
        }
        
        if let Some(ref connection_manager) = self.connection_manager {
            connection_manager.stop().await?;
        }
        
        info!("Client stopped successfully");
        Ok(())
    }
    
    pub fn get_stats(&self) -> ClientStats {
        let tunnels = self.tunnel_manager.list_tunnels();
        let running_tunnels = self.tunnel_manager.get_running_tunnels();
        
        ClientStats {
            total_tunnels: tunnels.len() as u32,
            running_tunnels: running_tunnels.len() as u32,
            total_connections: tunnels.iter().map(|t| t.active_connections).sum(),
            total_bytes_transferred: tunnels.iter().map(|t| t.bytes_transferred).sum(),
        }
    }
    
    pub fn list_tunnels(&self) -> Vec<crate::common::types::TunnelInfo> {
        self.tunnel_manager.list_tunnels()
    }
    
    pub async fn connection_state(&self) -> Option<crate::protocol::ConnectionState> {
        if let Some(ref connection_manager) = self.connection_manager {
            Some(connection_manager.connection_state().await)
        } else {
            None
        }
    }
}

/// Client statistics
#[derive(Debug, Clone)]
pub struct ClientStats {
    pub total_tunnels: u32,
    pub running_tunnels: u32,
    pub total_connections: u32,
    pub total_bytes_transferred: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let config = ClientConfig::default();
        let client = Client::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_stats() {
        let config = ClientConfig::default();
        let client = Client::new(config).unwrap();
        let stats = client.get_stats();
        assert_eq!(stats.total_tunnels, 0);
        assert_eq!(stats.running_tunnels, 0);
    }
}