//! セキュリティモジュール
//!
//! Conduitのセキュリティ基盤を提供します：
//! - Ed25519暗号化・署名システム
//! - TLS 1.3設定と管理
//! - 鍵管理・ローテーションシステム
//! - 認証・認可機能

pub mod crypto;
pub mod tls;
pub mod keys;
pub mod auth;

pub use crypto::{Ed25519KeyPair, Ed25519Signature, Ed25519Error};
pub use tls::{TlsConfig, TlsClientConfig, TlsServerConfig, TlsError};
pub use keys::{KeyManager, KeyRotationConfig, KeyError};
pub use auth::{AuthManager, AuthToken, AuthError};

use crate::common::error::Error;

/// セキュリティモジュール共通エラー型
#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("Crypto error: {0}")]
    Crypto(#[from] Ed25519Error),
    
    #[error("TLS error: {0}")]
    Tls(#[from] TlsError),
    
    #[error("Key management error: {0}")]
    Key(#[from] KeyError),
    
    #[error("Authentication error: {0}")]
    Auth(#[from] AuthError),
    
    #[error("Security configuration error: {message}")]
    Config { message: String },
    
    #[error("Security initialization error: {message}")]
    Initialization { message: String },
}

impl From<SecurityError> for Error {
    fn from(err: SecurityError) -> Self {
        Error::Security(err.to_string())
    }
}

/// セキュリティモジュールの結果型
pub type SecurityResult<T> = Result<T, SecurityError>;

/// セキュリティ設定
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SecurityConfig {
    /// 秘密鍵ファイルパス
    pub private_key_path: String,
    
    /// 公開鍵ファイルパス
    pub public_key_path: String,
    
    /// TLS設定
    pub tls: TlsConfig,
    
    /// 鍵ローテーション設定
    pub key_rotation: KeyRotationConfig,
    
    /// 認証設定
    pub auth_timeout_seconds: u64,
    
    /// セッション有効期限（秒）
    pub session_timeout_seconds: u64,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            private_key_path: "./keys/conduit.key".to_string(),
            public_key_path: "./keys/conduit.pub".to_string(),
            tls: TlsConfig::default(),
            key_rotation: KeyRotationConfig::default(),
            auth_timeout_seconds: 30,
            session_timeout_seconds: 3600, // 1時間
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_config_default() {
        let config = SecurityConfig::default();
        assert_eq!(config.private_key_path, "./keys/conduit.key");
        assert_eq!(config.public_key_path, "./keys/conduit.pub");
        assert_eq!(config.auth_timeout_seconds, 30);
        assert_eq!(config.session_timeout_seconds, 3600);
    }
}