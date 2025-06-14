//! Configuration management for Conduit
//!
//! This module handles loading and managing configuration from multiple sources:
//! CLI arguments > Environment variables > Configuration file > Defaults

use crate::common::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Router configuration
    pub router: RouterConfig,
    
    /// Security configuration
    pub security: SecurityConfig,
    
    /// Tunnel configurations
    pub tunnels: Vec<TunnelConfig>,
}

/// Router server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    /// Router server host
    pub host: String,
    
    /// Router server port
    pub port: u16,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Path to private key file
    pub private_key_path: PathBuf,
    
    /// Path to public key file (optional)
    pub public_key_path: Option<PathBuf>,
}

/// Individual tunnel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    /// Tunnel identifier name
    pub name: String,
    
    /// Source service address on router side
    pub source: String,
    
    /// Local bind address for incoming connections
    pub bind: String,
    
    /// Protocol (tcp or udp)
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

impl Config {
    /// Load configuration from file
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| Error::config(format!("Failed to read config file: {}", e)))?;
        
        let config: Config = toml::from_str(&content)
            .map_err(|e| Error::config(format!("Failed to parse config file: {}", e)))?;
        
        config.validate()?;
        Ok(config)
    }
    
    /// Save configuration to file
    pub fn to_file(&self, path: &PathBuf) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| Error::config(format!("Failed to serialize config: {}", e)))?;
        
        std::fs::write(path, content)
            .map_err(|e| Error::config(format!("Failed to write config file: {}", e)))?;
        
        Ok(())
    }
    
    /// Create a default configuration
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
    
    /// Generate sample configuration
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
    
    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate router configuration
        if self.router.host.is_empty() {
            return Err(Error::config("Router host cannot be empty"));
        }
        
        if self.router.port == 0 {
            return Err(Error::config("Router port cannot be 0"));
        }
        
        // Validate tunnels
        if self.tunnels.is_empty() {
            return Err(Error::config("At least one tunnel must be configured"));
        }
        
        let mut tunnel_names = std::collections::HashSet::new();
        let mut bind_addresses = std::collections::HashSet::new();
        
        for tunnel in &self.tunnels {
            // Check for duplicate tunnel names
            if !tunnel_names.insert(&tunnel.name) {
                return Err(Error::config(format!("Duplicate tunnel name: {}", tunnel.name)));
            }
            
            // Check for duplicate bind addresses
            if !bind_addresses.insert(&tunnel.bind) {
                return Err(Error::config(format!("Duplicate bind address: {}", tunnel.bind)));
            }
            
            // Validate addresses
            tunnel.source.parse::<SocketAddr>()
                .map_err(|_| Error::config(format!("Invalid source address: {}", tunnel.source)))?;
            
            tunnel.bind.parse::<SocketAddr>()
                .map_err(|_| Error::config(format!("Invalid bind address: {}", tunnel.bind)))?;
            
            // Validate protocol
            match tunnel.protocol.as_str() {
                "tcp" | "udp" => {},
                _ => return Err(Error::config(format!("Invalid protocol: {}", tunnel.protocol))),
            }
        }
        
        Ok(())
    }
    
    /// Get router socket address
    pub fn router_addr(&self) -> SocketAddr {
        format!("{}:{}", self.router.host, self.router.port)
            .parse()
            .expect("Invalid router address")
    }
}

impl RouterConfig {
    /// Create from environment variables
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
    /// Create from environment variables
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

/// Default protocol for tunnels
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