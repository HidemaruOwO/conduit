// プロトコルメッセージ定義
//
// Conduitのコア通信プロトコルメッセージを定義します：
// - JSON over TLSメッセージング
// - バージョニング対応
// - メッセージ検証機能

use std::net::SocketAddr;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageVersion {
    pub major: u32,
    pub minor: u32,
}

impl Default for MessageVersion {
    fn default() -> Self {
        Self { major: 1, minor: 0 }
    }
}

impl std::fmt::Display for MessageVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// メッセージタイプ
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageType {
    // Client -> Router
    ClientRegister,
    TunnelCreate,
    TunnelData,
    Heartbeat,
    
    // Router -> Client
    ClientRegisterResponse,
    TunnelCreateResponse,
    TunnelDataResponse,
    HeartbeatResponse,
    
    // 双方向
    Error,
    Disconnect,
}

/// ベースメッセージ構造
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// メッセージID
    pub id: Uuid,
    
    /// プロトコルバージョン
    pub version: MessageVersion,
    
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
    
    /// メッセージタイプ
    pub message_type: MessageType,
    
    /// メッセージペイロード
    pub payload: MessagePayload,
}

/// メッセージペイロード
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePayload {
    ClientRegister(ClientRegister),
    TunnelCreate(TunnelCreate),
    TunnelData(TunnelData),
    Heartbeat(Heartbeat),
    ClientRegisterResponse(ClientRegisterResponse),
    TunnelCreateResponse(TunnelCreateResponse),
    TunnelDataResponse(TunnelDataResponse),
    HeartbeatResponse(HeartbeatResponse),
    Error(ErrorMessage),
    Disconnect(DisconnectMessage),
}

// === Client -> Router メッセージ ===

/// クライアント登録要求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRegister {
    /// クライアントID
    pub client_id: Uuid,
    
    /// クライアント名
    pub client_name: String,
    
    /// クライアント公開鍵（Ed25519）
    pub public_key: String,
    
    /// 署名（認証用）
    pub signature: String,
    
    /// クライアントバージョン
    pub client_version: String,
    
    /// サポートする機能
    pub capabilities: Vec<String>,
}

/// トンネル作成要求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelCreate {
    /// トンネルID
    pub tunnel_id: Uuid,
    
    /// トンネル名
    pub tunnel_name: String,
    
    /// 転送先サービスアドレス（Router側）
    pub source_addr: SocketAddr,
    
    /// バインドアドレス（Client側）
    pub bind_addr: SocketAddr,
    
    /// プロトコル（TCP/UDP）
    pub protocol: String,
    
    /// トンネル設定
    pub config: TunnelConfig,
}

/// トンネル設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    /// 最大同時接続数
    pub max_connections: u32,
    
    /// タイムアウト（秒）
    pub timeout_seconds: u64,
    
    /// バッファサイズ（バイト）
    pub buffer_size: usize,
    
    /// 圧縮有効
    pub compression_enabled: bool,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            max_connections: 100,
            timeout_seconds: 300,
            buffer_size: 65536,
            compression_enabled: false,
        }
    }
}

/// トンネルデータ転送
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelData {
    /// トンネルID
    pub tunnel_id: Uuid,
    
    /// 接続ID
    pub connection_id: Uuid,
    
    /// データ（Base64エンコード）
    pub data: String,
    
    /// データサイズ（バイト）
    pub data_size: usize,
    
    /// シーケンス番号
    pub sequence: u64,
}

/// ハートビート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heartbeat {
    /// クライアントID
    pub client_id: Uuid,
    
    /// アクティブなトンネル数
    pub active_tunnels: u32,
    
    /// アクティブな接続数
    pub active_connections: u32,
    
    /// CPU使用率（0.0-1.0）
    pub cpu_usage: f32,
    
    /// メモリ使用量（バイト）
    pub memory_usage: u64,
}

// === Router -> Client レスポンス ===

/// クライアント登録レスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRegisterResponse {
    /// 登録成功
    pub success: bool,
    
    /// セッションID
    pub session_id: Option<Uuid>,
    
    /// サーバー公開鍵
    pub server_public_key: Option<String>,
    
    /// エラーメッセージ
    pub error: Option<String>,
    
    /// サーバー機能
    pub server_capabilities: Vec<String>,
}

/// トンネル作成レスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelCreateResponse {
    /// トンネルID
    pub tunnel_id: Uuid,
    
    /// 作成成功
    pub success: bool,
    
    /// ルーター側ポート
    pub router_port: Option<u16>,
    
    /// エラーメッセージ
    pub error: Option<String>,
}

/// トンネルデータレスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelDataResponse {
    /// トンネルID
    pub tunnel_id: Uuid,
    
    /// 接続ID
    pub connection_id: Uuid,
    
    /// レスポンスデータ（Base64エンコード）
    pub data: Option<String>,
    
    /// 受信確認シーケンス番号
    pub ack_sequence: u64,
    
    /// エラーメッセージ
    pub error: Option<String>,
}

/// ハートビートレスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    /// サーバー時刻
    pub server_time: DateTime<Utc>,
    
    /// 接続されているクライアント数
    pub connected_clients: u32,
    
    /// 総トンネル数
    pub total_tunnels: u32,
    
    /// サーバー負荷（0.0-1.0）
    pub server_load: f32,
}

// === 共通メッセージ ===

/// エラーメッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMessage {
    /// エラーコード
    pub code: String,
    
    /// エラーメッセージ
    pub message: String,
    
    /// 詳細情報
    pub details: Option<String>,
    
    /// 関連するメッセージID
    pub related_message_id: Option<Uuid>,
}

/// 切断メッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectMessage {
    /// 理由
    pub reason: String,
    
    /// 再接続可能
    pub reconnect_allowed: bool,
    
    /// 再接続待機時間（秒）
    pub reconnect_delay_seconds: Option<u64>,
}

/// プロトコルエラー
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Invalid message format: {message}")]
    InvalidFormat { message: String },
    
    #[error("Version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },
    
    #[error("Message too large: {size} > {max_size}")]
    MessageTooLarge { size: usize, max_size: usize },
    
    #[error("Invalid signature")]
    InvalidSignature,
    
    #[error("Timeout")]
    Timeout,
}

impl Message {
    /// 新しいメッセージを作成
    pub fn new(message_type: MessageType, payload: MessagePayload) -> Self {
        Self {
            id: Uuid::new_v4(),
            version: MessageVersion::default(),
            timestamp: Utc::now(),
            message_type,
            payload,
        }
    }
    
    /// JSONにシリアライズ
    pub fn to_json(&self) -> Result<String, ProtocolError> {
        serde_json::to_string(self).map_err(ProtocolError::from)
    }
    
    /// JSONからデシリアライズ
    pub fn from_json(json: &str) -> Result<Self, ProtocolError> {
        serde_json::from_str(json).map_err(ProtocolError::from)
    }
    
    /// メッセージサイズ（バイト）を取得
    pub fn size(&self) -> Result<usize, ProtocolError> {
        Ok(self.to_json()?.len())
    }
    
    /// メッセージを検証
    pub fn validate(&self, max_size: usize) -> Result<(), ProtocolError> {
        let size = self.size()?;
        if size > max_size {
            return Err(ProtocolError::MessageTooLarge {
                size,
                max_size,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_version() {
        let version = MessageVersion::default();
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 0);
        assert_eq!(version.to_string(), "1.0");
    }

    #[test]
    fn test_message_serialization() {
        let payload = MessagePayload::Heartbeat(Heartbeat {
            client_id: Uuid::new_v4(),
            active_tunnels: 2,
            active_connections: 5,
            cpu_usage: 0.25,
            memory_usage: 1024 * 1024,
        });
        
        let message = Message::new(MessageType::Heartbeat, payload);
        let json = message.to_json().unwrap();
        let deserialized = Message::from_json(&json).unwrap();
        
        assert_eq!(message.id, deserialized.id);
        assert_eq!(message.version, deserialized.version);
        // message_typeフィールドは削除されたため、payloadで判定
        match (&message.payload, &deserialized.payload) {
            (MessagePayload::Heartbeat(_), MessagePayload::Heartbeat(_)) => {},
            _ => panic!("Message payload type mismatch"),
        }
    }

    #[test]
    fn test_message_validation() {
        let payload = MessagePayload::Heartbeat(Heartbeat {
            client_id: Uuid::new_v4(),
            active_tunnels: 0,
            active_connections: 0,
            cpu_usage: 0.0,
            memory_usage: 0,
        });
        
        let message = Message::new(MessageType::Heartbeat, payload);
        
        // 十分なサイズ制限
        assert!(message.validate(1024 * 1024).is_ok());
        
        // 小さすぎるサイズ制限
        assert!(message.validate(100).is_err());
    }

    #[test]
    fn test_tunnel_config_default() {
        let config = TunnelConfig::default();
        assert_eq!(config.max_connections, 100);
        assert_eq!(config.timeout_seconds, 300);
        assert_eq!(config.buffer_size, 65536);
        assert!(!config.compression_enabled);
    }
}