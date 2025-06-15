//! Client設定管理
//!
//! セキュリティ設定統合、接続設定管理、プロファイル管理を提供します

use std::net::SocketAddr;
use std::path::PathBuf;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

use crate::security::SecurityConfig;
use crate::protocol::ProtocolConfig;
use crate::client::connection::ConnectionConfig;
use crate::common::error::Result;

/// Clientメイン設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Client基本情報
    pub client: ClientInfo,
    
    /// Router接続設定
    pub router: RouterConfig,
    
    /// セキュリティ設定
    pub security: SecurityConfig,
    
    /// プロトコル設定
    pub protocol: ProtocolConfig,
    
    /// 接続設定
    pub connection: ConnectionSettings,
    
    /// トンネル設定リスト
    pub tunnels: Vec<TunnelConfig>,
}

/// Client基本情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// Client ID
    pub id: Uuid,
    
    /// Client名
    pub name: String,
    
    /// Client説明
    pub description: Option<String>,
    
    /// Clientバージョン
    pub version: String,
}

impl Default for ClientInfo {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "conduit-client".to_string(),
            description: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Router接続設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    /// Router接続ホスト
    pub host: String,
    
    /// Router接続ポート
    pub port: u16,
    
    /// 接続タイムアウト（秒）
    pub connect_timeout_seconds: u64,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 9999,
            connect_timeout_seconds: 30,
        }
    }
}

/// 接続設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionSettings {
    /// 自動再接続有効
    pub auto_reconnect: bool,
    
    /// 再接続間隔（秒）
    pub reconnect_interval_seconds: u64,
    
    /// 最大再接続回数（0で無制限）
    pub max_reconnect_attempts: usize,
    
    /// ハートビート有効
    pub heartbeat_enabled: bool,
    
    /// ハートビート間隔（秒）
    pub heartbeat_interval_seconds: u64,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            auto_reconnect: true,
            reconnect_interval_seconds: 5,
            max_reconnect_attempts: 0,
            heartbeat_enabled: true,
            heartbeat_interval_seconds: 60,
        }
    }
}

/// トンネル設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    /// トンネル名
    pub name: String,
    
    /// 転送先サービスアドレス（Router側）
    pub source: SocketAddr,
    
    /// バインドアドレス（Client側）
    pub bind: SocketAddr,
    
    /// プロトコル（tcp/udp）
    pub protocol: String,
    
    /// 有効フラグ
    pub enabled: bool,
    
    /// トンネル固有設定
    pub settings: TunnelSettings,
}

/// トンネル固有設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelSettings {
    /// 最大同時接続数
    pub max_connections: u32,
    
    /// 接続タイムアウト（秒）
    pub connection_timeout_seconds: u64,
    
    /// バッファサイズ（バイト）
    pub buffer_size: usize,
    
    /// 圧縮有効
    pub compression_enabled: bool,
}

impl Default for TunnelSettings {
    fn default() -> Self {
        Self {
            max_connections: 100,
            connection_timeout_seconds: 30,
            buffer_size: 65536,
            compression_enabled: false,
        }
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            client: ClientInfo::default(),
            router: RouterConfig::default(),
            security: SecurityConfig::default(),
            protocol: ProtocolConfig::default(),
            connection: ConnectionSettings::default(),
            tunnels: Vec::new(),
        }
    }
}

impl ClientConfig {
    /// 設定ファイルから読み込み
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::common::error::Error::Config(format!("Failed to read config file: {}", e)))?;
        
        toml::from_str(&content)
            .map_err(|e| crate::common::error::Error::Config(format!("Failed to parse config file: {}", e)))
    }
    
    /// 設定ファイルに保存
    pub fn to_file(&self, path: &PathBuf) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| crate::common::error::Error::Config(format!("Failed to serialize config: {}", e)))?;
        
        std::fs::write(path, content)
            .map_err(|e| crate::common::error::Error::Config(format!("Failed to write config file: {}", e)))?;
        
        Ok(())
    }
    
    /// 設定を検証
    pub fn validate(&self) -> Result<()> {
        // Router設定の検証
        if self.router.host.is_empty() {
            return Err(crate::common::error::Error::Config("Router host cannot be empty".to_string()));
        }
        
        if self.router.port == 0 {
            return Err(crate::common::error::Error::Config("Router port must be greater than 0".to_string()));
        }
        
        // Client設定の検証
        if self.client.name.is_empty() {
            return Err(crate::common::error::Error::Config("Client name cannot be empty".to_string()));
        }
        
        // トンネル設定の検証
        for tunnel in &self.tunnels {
            if tunnel.name.is_empty() {
                return Err(crate::common::error::Error::Config("Tunnel name cannot be empty".to_string()));
            }
            
            if !matches!(tunnel.protocol.as_str(), "tcp" | "udp") {
                return Err(crate::common::error::Error::Config(
                    format!("Invalid tunnel protocol: {}", tunnel.protocol)
                ));
            }
        }
        
        Ok(())
    }
    
    /// ConnectionConfigに変換
    pub fn to_connection_config(&self) -> ConnectionConfig {
        ConnectionConfig {
            router_addr: format!("{}:{}", self.router.host, self.router.port),
            client_id: self.client.id,
            client_name: self.client.name.clone(),
            auto_reconnect: self.connection.auto_reconnect,
            reconnect_interval_seconds: self.connection.reconnect_interval_seconds,
            max_reconnect_attempts: self.connection.max_reconnect_attempts,
            connection_timeout_seconds: self.router.connect_timeout_seconds,
            heartbeat_enabled: self.connection.heartbeat_enabled,
            heartbeat_interval_seconds: self.connection.heartbeat_interval_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_client_config_default() {
        let config = ClientConfig::default();
        assert_eq!(config.client.name, "conduit-client");
        assert_eq!(config.router.host, "0.0.0.0");
        assert_eq!(config.router.port, 9999);
        assert!(config.connection.auto_reconnect);
    }

    #[test]
    fn test_config_validation() {
        let mut config = ClientConfig::default();
        assert!(config.validate().is_ok());
        
        // 無効な設定をテスト
        config.router.host = "".to_string();
        assert!(config.validate().is_err());
        
        config.router.host = "localhost".to_string();
        config.router.port = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_file_operations() {
        let config = ClientConfig::default();
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");
        
        // 保存
        assert!(config.to_file(&config_path).is_ok());
        
        // 読み込み
        let loaded_config = ClientConfig::from_file(&config_path).unwrap();
        assert_eq!(config.client.name, loaded_config.client.name);
        assert_eq!(config.router.host, loaded_config.router.host);
    }

    #[test]
    fn test_connection_config_conversion() {
        let client_config = ClientConfig::default();
        let connection_config = client_config.to_connection_config();
        
        assert_eq!(connection_config.client_id, client_config.client.id);
        assert_eq!(connection_config.client_name, client_config.client.name);
        assert_eq!(connection_config.router_addr, "0.0.0.0:9999");
    }
}