// Process Registry データモデル
// Podmanライクな数値状態管理システム

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;
use std::path::PathBuf;

// Podmanライクなトンネル状態（数値管理）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum TunnelStatus {
    Created = 1,   
    Running = 3,   
    Stopping = 4,  
    Exited = 5,    
    Error = 6,     
}

impl TunnelStatus {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            1 => Some(Self::Created),
            3 => Some(Self::Running),
            4 => Some(Self::Stopping),
            5 => Some(Self::Exited),
            6 => Some(Self::Error),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Exited => "exited",
            Self::Error => "error",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Stopping)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    pub router_addr: String,     
    pub source_addr: String,     
    pub bind_addr: String,       
    pub protocol: String,        
    pub timeout_seconds: u32,    
    pub max_connections: u32,    
}

// SQLiteデータベースレコード構造体
#[derive(Debug, Clone, FromRow)]
pub struct TunnelEntry {
    pub id: String,                    
    pub name: String,                  
    pub pid: Option<i32>,              
    pub socket_path_hash: String,      
    pub status: i32,                   
    pub config_encrypted: Option<Vec<u8>>, 
    pub config_checksum: String,       
    pub created_at: i64,               
    pub updated_at: i64,               
    pub last_activity: i64,            
    pub exit_code: Option<i32>,        
}

impl TunnelEntry {
    pub fn new(
        id: String,
        name: String,
        pid: i32,
        socket_path: &str,
        config: &TunnelConfig,
        encryption_key: &[u8],
    ) -> anyhow::Result<Self> {
        let now = chrono::Utc::now().timestamp();
        let socket_path_hash = Self::hash_path(socket_path)?;
        let config_json = serde_json::to_string(config)?;
        let config_encrypted = Self::encrypt_config(&config_json, encryption_key)?;
        let config_checksum = Self::compute_checksum(&config_json)?;

        Ok(Self {
            id,
            name,
            pid: Some(pid),
            socket_path_hash,
            status: TunnelStatus::Created as i32,
            config_encrypted: Some(config_encrypted),
            config_checksum,
            created_at: now,
            updated_at: now,
            last_activity: now,
            exit_code: None,
        })
    }

    pub fn get_status(&self) -> TunnelStatus {
        TunnelStatus::from_i32(self.status).unwrap_or(TunnelStatus::Error)
    }

    // 設定データの完全性チェック付き復号化
    pub fn decrypt_config(&self, encryption_key: &[u8]) -> anyhow::Result<TunnelConfig> {
        let encrypted_data = self.config_encrypted.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No encrypted config found"))?;
        
        let config_json = Self::decrypt_data(encrypted_data, encryption_key)?;
        let config: TunnelConfig = serde_json::from_str(&config_json)?;
        
        // データ改ざん検知のためのハッシュ値検証
        let computed_checksum = Self::compute_checksum(&config_json)?;
        if computed_checksum != self.config_checksum {
            return Err(anyhow::anyhow!("Config integrity check failed"));
        }
        
        Ok(config)
    }

    // セキュリティのためパス情報をハッシュ化
    fn hash_path(path: &str) -> anyhow::Result<String> {
        use ring::digest::{Context, SHA256};
        let mut context = Context::new(&SHA256);
        context.update(path.as_bytes());
        let digest = context.finish();
        Ok(base64::engine::general_purpose::STANDARD.encode(digest.as_ref()))
    }

    // AES-256-GCMによる機密データの暗号化
    fn encrypt_config(config_json: &str, key: &[u8]) -> anyhow::Result<Vec<u8>> {
        use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
        use ring::rand::{SecureRandom, SystemRandom};

        let unbound_key = UnboundKey::new(&AES_256_GCM, key)?;
        let key = LessSafeKey::new(unbound_key);
        
        let rng = SystemRandom::new();
        let mut nonce_bytes = [0u8; 12];
        rng.fill(&mut nonce_bytes)?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        
        let mut in_out = config_json.as_bytes().to_vec();
        key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)?;
        
        // nonceとciphertextを結合して保存
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&in_out);
        
        Ok(result)
    }

    fn decrypt_data(encrypted_data: &[u8], key: &[u8]) -> anyhow::Result<String> {
        use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};

        if encrypted_data.len() < 12 {
            return Err(anyhow::anyhow!("Invalid encrypted data length"));
        }

        let (nonce_bytes, ciphertext) = encrypted_data.split_at(12);
        let nonce = Nonce::assume_unique_for_key(
            nonce_bytes.try_into().map_err(|_| anyhow::anyhow!("Invalid nonce length"))?
        );

        let unbound_key = UnboundKey::new(&AES_256_GCM, key)?;
        let key = LessSafeKey::new(unbound_key);
        
        let mut in_out = ciphertext.to_vec();
        let plaintext = key.open_in_place(nonce, Aad::empty(), &mut in_out)?;
        
        Ok(String::from_utf8(plaintext.to_vec())?)
    }

    // データ完全性チェック用のSHA-256ハッシュ
    fn compute_checksum(data: &str) -> anyhow::Result<String> {
        use ring::digest::{Context, SHA256};
        let mut context = Context::new(&SHA256);
        context.update(data.as_bytes());
        let digest = context.finish();
        Ok(base64::engine::general_purpose::STANDARD.encode(digest.as_ref()))
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct ClientEntry {
    pub id: String,                    
    pub tunnel_id: String,             
    pub client_addr_hash: String,      
    pub target_addr_hash: String,      
    pub connected_at: i64,             
    pub disconnected_at: Option<i64>,  
    pub last_activity: i64,            
    pub bytes_sent: i64,               
    pub bytes_received: i64,           
    pub status: String,                
    pub session_timeout: i32,          
}

#[derive(Debug, Clone, FromRow)]
pub struct SessionEntry {
    pub id: String,                    
    pub tunnel_id: String,             
    pub started_at: i64,               
    pub ended_at: Option<i64>,         
    pub total_connections: i32,        
    pub total_bytes_sent: i64,         
    pub total_bytes_received: i64,     
    pub avg_latency_ms: f64,           
    pub error_count: i32,              
}

// 公開API用の復号化済みデータ構造
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelInfo {
    pub id: String,
    pub name: String,
    pub pid: Option<u32>,
    pub socket_path: PathBuf,          
    pub status: TunnelStatus,
    pub config: TunnelConfig,          
    pub created_at: i64,
    pub updated_at: i64,
    pub last_activity: i64,
    pub exit_code: Option<i32>,
    pub metrics: TunnelMetrics,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TunnelMetrics {
    pub active_connections: u32,       
    pub total_connections: u64,        
    pub total_bytes_sent: u64,         
    pub total_bytes_received: u64,     
    pub cpu_usage: f64,                
    pub memory_usage: u64,             
    pub uptime_seconds: u64,           
    pub avg_latency_ms: f64,           
    pub error_rate: f64,               
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub id: String,
    pub tunnel_id: String,
    pub client_addr: String,           
    pub target_addr: String,           
    pub connected_at: i64,
    pub disconnected_at: Option<i64>,
    pub last_activity: i64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub status: String,
    pub session_timeout: u32,
}

#[derive(Debug, Clone, FromRow)]
pub struct AuditLogEntry {
    pub id: i64,
    pub action: String,                
    pub target_table: String,          
    pub target_id: Option<String>,     
    pub user_context: Option<String>,  
    pub timestamp: i64,                
    pub success: bool,                 
    pub error_message: Option<String>, 
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tunnel_status_conversion() {
        assert_eq!(TunnelStatus::from_i32(1), Some(TunnelStatus::Created));
        assert_eq!(TunnelStatus::from_i32(3), Some(TunnelStatus::Running));
        assert_eq!(TunnelStatus::from_i32(999), None);
    }

    #[test]
    fn test_tunnel_status_is_active() {
        assert!(!TunnelStatus::Created.is_active());
        assert!(TunnelStatus::Running.is_active());
        assert!(TunnelStatus::Stopping.is_active());
        assert!(!TunnelStatus::Exited.is_active());
    }

    #[test]
    fn test_encryption_decryption() {
        let config = TunnelConfig {
            router_addr: "10.2.0.1:9999".to_string(),
            source_addr: "10.2.0.2:8080".to_string(),
            bind_addr: "0.0.0.0:80".to_string(),
            protocol: "tcp".to_string(),
            timeout_seconds: 30,
            max_connections: 100,
        };

        let key = b"0123456789abcdef0123456789abcdef"; // 32 bytes
        let entry = TunnelEntry::new(
            "test-id".to_string(),
            "test-tunnel".to_string(),
            12345,
            "/tmp/test.sock",
            &config,
            key,
        ).unwrap();

        let decrypted_config = entry.decrypt_config(key).unwrap();
        assert_eq!(decrypted_config.router_addr, config.router_addr);
        assert_eq!(decrypted_config.source_addr, config.source_addr);
    }
}