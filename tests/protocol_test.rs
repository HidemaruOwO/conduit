// プロトコルテスト
//
// Router-Client間のコア通信プロトコルの基本テスト

use conduit::protocol::{Message, MessageType, MessagePayload};
use conduit::protocol::messages::{Heartbeat, ClientRegister};
use uuid::Uuid;

#[tokio::test]
async fn test_basic_message_serialization() {
    let heartbeat = MessagePayload::Heartbeat(Heartbeat {
        client_id: Uuid::new_v4(),
        active_tunnels: 2,
        active_connections: 5,
        cpu_usage: 0.25,
        memory_usage: 1024 * 1024,
    });
    
    let message = Message::new(MessageType::Heartbeat, heartbeat);
    
    // JSON形式でのシリアライゼーション・デシリアライゼーション検証
    let json = message.to_json().unwrap();
    let deserialized = Message::from_json(&json).unwrap();
    
    assert_eq!(message.id, deserialized.id);
    assert_eq!(message.version, deserialized.version);
    assert_eq!(message.message_type, deserialized.message_type);
}

#[tokio::test]
async fn test_client_register_message() {
    let register = MessagePayload::ClientRegister(ClientRegister {
        client_id: Uuid::new_v4(),
        client_name: "test-client".to_string(),
        public_key: "test-public-key".to_string(),
        signature: "test-signature".to_string(),
        client_version: "1.0.0".to_string(),
        capabilities: vec!["tcp".to_string(), "heartbeat".to_string()],
    });
    
    let message = Message::new(MessageType::ClientRegister, register);
    
    // 正常なサイズ制限でのバリデーション
    assert!(message.validate(1024 * 1024).is_ok());
    
    // サイズ制限超過時のエラーハンドリング検証
    assert!(message.validate(100).is_err());
}