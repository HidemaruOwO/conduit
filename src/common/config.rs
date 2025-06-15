// Conduitの設定管理
//
// 複数のソースから設定を読み込み管理：
// CLI引数 > 環境変数 > 設定ファイル > デフォルト値

use crate::common::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

// メイン設定構造体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub router: RouterConfig,
    pub security: SecurityConfig,
    pub tunnels: Vec<TunnelConfig>,
}

// Routerサーバー設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    pub host: String,
    pub port: u16,
}

// セキュリティ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub private_key_path: PathBuf,
    pub public_key_path: Option<PathBuf>,
}

// 個別トンネル設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    pub name: String,
    // Router側のサービスアドレス
    pub source: String,
    // Clientのローカルバインドアドレス
    pub bind: String,
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

impl Config {
    // ファイルから設定を読み込み
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| Error::config(format!("Failed to read config file: {}", e)))?;
        
        let config: Config = toml::from_str(&content)
            .map_err(|e| Error::config(format!("Failed to parse config file: {}", e)))?;
        
        config.validate()?;
        Ok(config)
    }
    
    // 設定をファイルに保存
    pub fn to_file(&self, path: &PathBuf) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| Error::config(format!("Failed to serialize config: {}", e)))?;
        
        std::fs::write(path, content)
            .map_err(|e| Error::config(format!("Failed to write config file: {}", e)))?;
        
        Ok(())
    }
    
    // デフォルト設定を作成
    pub fn default() -> Self {
        Config {
            router: RouterConfig {
                host: "localhost".to_string(),
                port: 9999,
            },
            security: SecurityConfig {
                private_key_path: PathBuf::from("./keys/client.key"),
                public_key_path: Some(PathBuf::from("./keys/client.pub")),
            },
            tunnels: vec![
                TunnelConfig {
                    name: "example-tunnel".to_string(),
                    source: "127.0.0.1:8080".to_string(),
                    bind: "0.0.0.0:80".to_string(),
                    protocol: "tcp".to_string(),
                }
            ],
        }
    }
    
    // サンプル設定を生成（architecture.mdの仕様に準拠）
    pub fn sample() -> Self {
        Config {
            router: RouterConfig {
                host: "10.2.0.1".to_string(),
                port: 9999,
            },
            security: SecurityConfig {
                private_key_path: PathBuf::from("./keys/client.key"),
                public_key_path: Some(PathBuf::from("./keys/client.pub")),
            },
            tunnels: vec![
                TunnelConfig {
                    name: "web-server-access".to_string(),
                    source: "10.2.0.2:8080".to_string(),
                    bind: "0.0.0.0:80".to_string(),
                    protocol: "tcp".to_string(),
                },
                TunnelConfig {
                    name: "api-server-access".to_string(),
                    source: "10.2.0.3:3000".to_string(),
                    bind: "0.0.0.0:8080".to_string(),
                    protocol: "tcp".to_string(),
                },
            ],
        }
    }
    
    // 設定の妥当性を検証
    pub fn validate(&self) -> Result<()> {
        if self.router.host.is_empty() {
            return Err(Error::config("Router host cannot be empty"));
        }
        
        if self.router.port == 0 {
            return Err(Error::config("Router port cannot be 0"));
        }
        
        if self.tunnels.is_empty() {
            return Err(Error::config("At least one tunnel must be configured"));
        }
        
        let mut tunnel_names = std::collections::HashSet::new();
        let mut bind_addresses = std::collections::HashSet::new();
        
        for tunnel in &self.tunnels {
            // 重複チェック：トンネル名
            if !tunnel_names.insert(&tunnel.name) {
                return Err(Error::config(format!("Duplicate tunnel name: {}", tunnel.name)));
            }
            
            // 重複チェック：バインドアドレス
            if !bind_addresses.insert(&tunnel.bind) {
                return Err(Error::config(format!("Duplicate bind address: {}", tunnel.bind)));
            }
            
            // アドレス形式の検証
            tunnel.source.parse::<SocketAddr>()
                .map_err(|_| Error::config(format!("Invalid source address: {}", tunnel.source)))?;
            
            tunnel.bind.parse::<SocketAddr>()
                .map_err(|_| Error::config(format!("Invalid bind address: {}", tunnel.bind)))?;
            
            // プロトコルの検証
            match tunnel.protocol.as_str() {
                "tcp" | "udp" => {},
                _ => return Err(Error::config(format!("Invalid protocol: {}", tunnel.protocol))),
            }
        }
        
        Ok(())
    }
    
    // RouterのSocketAddrを取得
    pub fn router_addr(&self) -> SocketAddr {
        format!("{}:{}", self.router.host, self.router.port)
            .parse()
            .expect("Invalid router address")
    }
}

impl RouterConfig {
    // 環境変数からRouter設定を作成
    pub fn from_env() -> Self {
        RouterConfig {
            host: std::env::var("CONDUIT_ROUTER_HOST")
                .unwrap_or_else(|_| "localhost".to_string()),
            port: std::env::var("CONDUIT_ROUTER_PORT")
                .unwrap_or_else(|_| "9999".to_string())
                .parse()
                .unwrap_or(9999),
        }
    }
}

impl SecurityConfig {
    // 環境変数からセキュリティ設定を作成
    pub fn from_env() -> Self {
        SecurityConfig {
            private_key_path: std::env::var("CONDUIT_SECURITY_PRIVATE_KEY_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("./keys/client.key")),
            public_key_path: std::env::var("CONDUIT_SECURITY_PUBLIC_KEY_PATH")
                .map(PathBuf::from)
                .ok(),
        }
    }
}

// トンネルのデフォルトプロトコル
fn default_protocol() -> String {
    "tcp".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    
    #[test]
    fn test_config_validation() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_config_serialization() {
        let config = Config::sample();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.router.host, parsed.router.host);
    }
}