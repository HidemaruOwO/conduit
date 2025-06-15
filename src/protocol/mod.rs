//! コア通信プロトコルモジュール
//!
//! Conduitのコア通信プロトコルを提供します：
//! - JSON over TLSメッセージング
//! - Client-Router間のセキュア通信
//! - バージョニング対応
//! - メッセージ検証とエラーハンドリング

pub mod messages;
pub mod handler;
pub mod codec;

pub use messages::{
    Message, MessageType, MessageVersion, MessagePayload, ProtocolError,
    ClientRegister, TunnelCreate, TunnelData, Heartbeat,
    ClientRegisterResponse, TunnelCreateResponse, TunnelDataResponse, HeartbeatResponse,
};
pub use handler::{ProtocolHandler, ProtocolHandlerConfig, ConnectionState};
pub use codec::{MessageCodec, CodecError};

use crate::common::error::Error;

/// プロトコルモジュール共通エラー型
#[derive(Debug, thiserror::Error)]
pub enum ProtocolModuleError {
    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    
    #[error("Codec error: {0}")]
    Codec(#[from] CodecError),
    
    #[error("Handler error: {message}")]
    Handler { message: String },
    
    #[error("Version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },
    
    #[error("Invalid message format: {message}")]
    InvalidFormat { message: String },
}

impl From<ProtocolModuleError> for Error {
    fn from(err: ProtocolModuleError) -> Self {
        Error::Protocol(err.to_string())
    }
}

/// プロトコルモジュールの結果型
pub type ProtocolResult<T> = Result<T, ProtocolModuleError>;

/// プロトコル設定
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProtocolConfig {
    /// プロトコルバージョン
    pub version: String,
    
    /// メッセージタイムアウト（秒）
    pub message_timeout_seconds: u64,
    
    /// ハートビート間隔（秒）
    pub heartbeat_interval_seconds: u64,
    
    /// 最大メッセージサイズ（バイト）
    pub max_message_size: usize,
    
    /// 再試行回数
    pub max_retries: usize,
    
    /// 再試行間隔（ミリ秒）
    pub retry_delay_ms: u64,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            message_timeout_seconds: 30,
            heartbeat_interval_seconds: 60,
            max_message_size: 1024 * 1024, // 1MB
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_config_default() {
        let config = ProtocolConfig::default();
        assert_eq!(config.version, "1.0");
        assert_eq!(config.message_timeout_seconds, 30);
        assert_eq!(config.heartbeat_interval_seconds, 60);
        assert_eq!(config.max_message_size, 1024 * 1024);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
    }
}