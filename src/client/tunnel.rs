// Clientのトンネル実装

use crate::common::{error::Result, types::*};
use crate::protocol::{ProtocolHandler, Message, MessageType, MessagePayload, TunnelCreate};
use crate::security::{TlsClientConfig, AuthManager};
use crate::client::connection::ConnectionManager;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use tracing::{debug, info, warn, error, instrument};

pub struct TunnelManager {
    tunnels: dashmap::DashMap<TunnelId, TunnelInfo>,
    connection_manager: Arc<Mutex<Option<ConnectionManager>>>,
    tls_config: TlsClientConfig,
    auth_manager: Arc<AuthManager>,
}

impl TunnelManager {
    pub fn new(
        tls_config: TlsClientConfig,
        auth_manager: Arc<AuthManager>,
    ) -> Self {
        Self {
            tunnels: dashmap::DashMap::new(),
            connection_manager: Arc::new(Mutex::new(None)),
            tls_config,
            auth_manager,
        }
    }
    
    /// Set connection manager
    pub async fn set_connection_manager(&self, connection_manager: ConnectionManager) {
        *self.connection_manager.lock().await = Some(connection_manager);
    }
    
    /// Create a new tunnel with protocol support
    #[instrument(skip(self))]
    pub async fn create_tunnel(
        &self,
        name: String,
        source: SocketAddr,
        bind: SocketAddr,
        router: SocketAddr,
        protocol: Protocol,
    ) -> Result<TunnelId> {
        info!("Creating tunnel: {}", name);
        
        let tunnel_id = TunnelId::new();
        let tunnel_info = TunnelInfo {
            id: tunnel_id.clone(),
            name: name.clone(),
            source,
            bind,
            router,
            protocol: protocol.clone(),
            status: TunnelStatus::Starting,
            active_connections: 0,
            bytes_transferred: 0,
            created_at: chrono::Utc::now(),
        };
        
        self.tunnels.insert(tunnel_id.clone(), tunnel_info);
        
        // Router にトンネル作成要求を送信
        if let Err(e) = self.send_tunnel_create_request(&tunnel_id, &name, source, bind, &protocol).await {
            error!("Failed to send tunnel create request: {}", e);
            // エラー時はトンネル情報を削除
            self.tunnels.remove(&tunnel_id);
            return Err(e);
        }
        
        // トンネル状態を実行中に更新
        if let Some(mut tunnel_info) = self.tunnels.get_mut(&tunnel_id) {
            tunnel_info.status = TunnelStatus::Active;
        }
        
        info!("Tunnel created successfully: {}", name);
        Ok(tunnel_id)
    }
    
    /// Router にトンネル作成要求を送信
    async fn send_tunnel_create_request(
        &self,
        tunnel_id: &TunnelId,
        name: &str,
        source: SocketAddr,
        bind: SocketAddr,
        protocol: &Protocol,
    ) -> Result<()> {
        let connection_guard = self.connection_manager.lock().await;
        
        if let Some(ref connection_manager) = *connection_guard {
            let tunnel_create = MessagePayload::TunnelCreate(TunnelCreate {
                tunnel_id: Uuid::parse_str(&tunnel_id.to_string())
                    .map_err(|e| crate::common::error::Error::Protocol(format!("Invalid tunnel ID: {}", e)))?,
                tunnel_name: name.to_string(),
                source_addr: source,
                bind_addr: bind,
                protocol: protocol.to_string(),
                config: crate::protocol::messages::TunnelConfig {
                    max_connections: 100,
                    timeout_seconds: 300,
                    buffer_size: 65536,
                    compression_enabled: false,
                },
            });
            
            let message = Message::new(MessageType::TunnelCreate, tunnel_create);
            
            let response = connection_manager.send_message(message).await?;
            
            match response.payload {
                MessagePayload::TunnelCreateResponse(ref resp) => {
                    if resp.success {
                        debug!("Tunnel create response received: {}", resp.tunnel_id);
                        Ok(())
                    } else {
                        let error_msg = resp.error.as_deref().unwrap_or("Unknown error");
                        Err(crate::common::error::Error::Network(
                            format!("Router rejected tunnel creation: {}", error_msg)
                        ))
                    }
                }
                _ => Err(crate::common::error::Error::Protocol(
                    "Unexpected response type for tunnel create".to_string()
                )),
            }
        } else {
            Err(crate::common::error::Error::Network(
                "No connection manager available".to_string()
            ))
        }
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
    #[instrument(skip(self))]
    pub async fn remove_tunnel(&self, tunnel_id: &TunnelId) -> Result<()> {
        info!("Removing tunnel: {}", tunnel_id);
        
        if let Some((_, mut tunnel_info)) = self.tunnels.remove(tunnel_id) {
            tunnel_info.status = TunnelStatus::Stopping;
            
            // Router にトンネル削除要求を送信（実装時に追加）
            // TODO: Implement tunnel deletion request to router
            
            debug!("Tunnel removed: {}", tunnel_id);
        }
        Ok(())
    }
    
    /// Update tunnel statistics
    pub fn update_tunnel_stats(&self, tunnel_id: &TunnelId, active_connections: u32, bytes_transferred: u64) {
        if let Some(mut tunnel_info) = self.tunnels.get_mut(tunnel_id) {
            tunnel_info.active_connections = active_connections;
            tunnel_info.bytes_transferred = bytes_transferred;
        }
    }
    
    /// Get tunnel count by status
    pub fn get_tunnel_count_by_status(&self, status: TunnelStatus) -> usize {
        self.tunnels
            .iter()
            .filter(|entry| entry.value().status == status)
            .count()
    }
    
    /// Check if tunnel exists
    pub fn tunnel_exists(&self, tunnel_id: &TunnelId) -> bool {
        self.tunnels.contains_key(tunnel_id)
    }
    
    /// Get running tunnels
    pub fn get_running_tunnels(&self) -> Vec<TunnelInfo> {
        self.tunnels
            .iter()
            .filter(|entry| entry.value().status == TunnelStatus::Active)
            .map(|entry| entry.value().clone())
            .collect()
    }
}

impl Default for TunnelManager {
    fn default() -> Self {
        // デフォルト実装では空のTLS設定とAuthManagerを使用
        let tls_config = TlsClientConfig::new(&crate::security::TlsConfig::default())
            .expect("Failed to create default TLS config");
        let key_manager = crate::security::KeyManager::new(
            "./keys",
            crate::security::KeyRotationConfig::default()
        ).expect("Failed to create key manager");
        let session_timeout = std::time::Duration::from_secs(3600);
        let token_duration = std::time::Duration::from_secs(1800);
        let auth_manager = Arc::new(crate::security::AuthManager::new(key_manager, session_timeout, token_duration));
        
        Self::new(tls_config, auth_manager)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{TlsConfig, AuthManager};

    fn create_test_tunnel_manager() -> TunnelManager {
        let tls_config = TlsClientConfig::new(TlsConfig::default()).unwrap();
        let auth_manager = Arc::new(AuthManager::new());
        TunnelManager::new(tls_config, auth_manager)
    }

    #[test]
    fn test_tunnel_manager_creation() {
        let manager = create_test_tunnel_manager();
        assert_eq!(manager.list_tunnels().len(), 0);
    }

    #[tokio::test]
    async fn test_tunnel_creation_without_connection() {
        let manager = create_test_tunnel_manager();
        
        let result = manager.create_tunnel(
            "test-tunnel".to_string(),
            "127.0.0.1:8080".parse().unwrap(),
            "0.0.0.0:80".parse().unwrap(),
            "127.0.0.1:9999".parse().unwrap(),
            Protocol::Tcp,
        ).await;
        
        // 接続マネージャーがないので失敗するはず
        assert!(result.is_err());
    }

    #[test]
    fn test_tunnel_manager_operations() {
        let manager = create_test_tunnel_manager();
        
        // 空の状態をテスト
        assert_eq!(manager.get_tunnel_count_by_status(TunnelStatus::Running), 0);
        assert_eq!(manager.get_running_tunnels().len(), 0);
        
        let fake_tunnel_id = TunnelId::new();
        assert!(!manager.tunnel_exists(&fake_tunnel_id));
        assert!(manager.get_tunnel(&fake_tunnel_id).is_none());
    }
}