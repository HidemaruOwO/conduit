// メッセージコーデック
//
// JSON over TLSプロトコルのバイナリエンコーディングを提供します：
// - 4バイト長プレフィックス + JSONペイロード
// - 非同期ストリーム対応
// - エラーハンドリング

use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::protocol::messages::{Message, ProtocolError};

/// コーデックエラー
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    
    #[error("Message too large: {size} > {max_size}")]
    MessageTooLarge { size: u32, max_size: u32 },
    
    #[error("Invalid message length: {length}")]
    InvalidLength { length: u32 },
    
    #[error("Connection closed")]
    ConnectionClosed,
    
    #[error("Encoding error: {message}")]
    Encoding { message: String },
    
    #[error("Decoding error: {message}")]
    Decoding { message: String },
}

/// メッセージコーデック
pub struct MessageCodec {
    /// 最大メッセージサイズ（バイト）
    max_message_size: u32,
}

impl MessageCodec {
    /// 新しいコーデックを作成
    pub fn new(max_message_size: u32) -> Self {
        Self { max_message_size }
    }
    
    /// メッセージをエンコードしてバイナリ形式に変換
    pub fn encode(&self, message: &Message) -> Result<Vec<u8>, CodecError> {
        // JSONにシリアライズ
        let json = message.to_json().map_err(|e| CodecError::Encoding {
            message: e.to_string(),
        })?;
        
        let json_bytes = json.as_bytes();
        let length = json_bytes.len() as u32;
        
        // サイズチェック
        if length > self.max_message_size {
            return Err(CodecError::MessageTooLarge {
                size: length,
                max_size: self.max_message_size,
            });
        }
        
        // 4バイト長プレフィックス + JSONペイロード
        let mut encoded = Vec::with_capacity(4 + json_bytes.len());
        encoded.extend_from_slice(&length.to_be_bytes());
        encoded.extend_from_slice(json_bytes);
        
        Ok(encoded)
    }
    
    /// バイナリ形式からメッセージをデコード
    pub fn decode(&self, data: &[u8]) -> Result<Message, CodecError> {
        if data.len() < 4 {
            return Err(CodecError::InvalidLength {
                length: data.len() as u32,
            });
        }
        
        // 長さプレフィックスを読み取り
        let length_bytes: [u8; 4] = data[0..4].try_into().map_err(|_| {
            CodecError::InvalidLength {
                length: data.len() as u32,
            }
        })?;
        let length = u32::from_be_bytes(length_bytes);
        
        // サイズチェック
        if length > self.max_message_size {
            return Err(CodecError::MessageTooLarge {
                size: length,
                max_size: self.max_message_size,
            });
        }
        
        // JSONペイロード部分のチェック
        let expected_total_length = 4 + length as usize;
        if data.len() < expected_total_length {
            return Err(CodecError::InvalidLength {
                length: data.len() as u32,
            });
        }
        
        // JSONデータを抽出
        let json_data = &data[4..expected_total_length];
        let json = std::str::from_utf8(json_data).map_err(|e| {
            CodecError::Decoding {
                message: format!("Invalid UTF-8: {}", e),
            }
        })?;
        
        // メッセージをデシリアライズ
        Message::from_json(json).map_err(|e| CodecError::Decoding {
            message: e.to_string(),
        })
    }
    
    /// ストリームからメッセージを非同期で読み取り
    pub async fn read_message<R>(&self, reader: &mut R) -> Result<Message, CodecError>
    where
        R: AsyncReadExt + Unpin,
    {
        // 長さプレフィックス（4バイト）を読み取り
        let mut length_bytes = [0u8; 4];
        match reader.read_exact(&mut length_bytes).await {
            Ok(_) => {},
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(CodecError::ConnectionClosed);
            },
            Err(e) => return Err(CodecError::Io(e)),
        }
        
        let length = u32::from_be_bytes(length_bytes);
        
        // サイズチェック
        if length > self.max_message_size {
            return Err(CodecError::MessageTooLarge {
                size: length,
                max_size: self.max_message_size,
            });
        }
        
        if length == 0 {
            return Err(CodecError::InvalidLength { length });
        }
        
        // JSONペイロードを読み取り
        let mut json_buffer = vec![0u8; length as usize];
        match reader.read_exact(&mut json_buffer).await {
            Ok(_) => {},
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(CodecError::ConnectionClosed);
            },
            Err(e) => return Err(CodecError::Io(e)),
        }
        
        // JSONを文字列に変換
        let json = std::str::from_utf8(&json_buffer).map_err(|e| {
            CodecError::Decoding {
                message: format!("Invalid UTF-8: {}", e),
            }
        })?;
        
        // メッセージをデシリアライズ
        Message::from_json(json).map_err(|e| CodecError::Decoding {
            message: e.to_string(),
        })
    }
    
    /// ストリームにメッセージを非同期で書き込み
    pub async fn write_message<W>(&self, writer: &mut W, message: &Message) -> Result<(), CodecError>
    where
        W: AsyncWriteExt + Unpin,
    {
        let encoded = self.encode(message)?;
        writer.write_all(&encoded).await?;
        writer.flush().await?;
        Ok(())
    }
    
    /// バッファから完全なメッセージを抽出（バッファリング用）
    pub fn extract_message(&self, buffer: &mut Vec<u8>) -> Result<Option<Message>, CodecError> {
        if buffer.len() < 4 {
            return Ok(None); // 長さプレフィックスが不完全
        }
        
        // 長さプレフィックスを読み取り
        let length_bytes: [u8; 4] = buffer[0..4].try_into().unwrap();
        let length = u32::from_be_bytes(length_bytes);
        
        // サイズチェック
        if length > self.max_message_size {
            return Err(CodecError::MessageTooLarge {
                size: length,
                max_size: self.max_message_size,
            });
        }
        
        let total_length = 4 + length as usize;
        
        if buffer.len() < total_length {
            return Ok(None); // メッセージが不完全
        }
        
        // 完全なメッセージデータを抽出
        let message_data = buffer.drain(0..total_length).collect::<Vec<u8>>();
        
        // メッセージをデコード
        self.decode(&message_data).map(Some)
    }
}

impl Default for MessageCodec {
    fn default() -> Self {
        Self::new(1024 * 1024) // 1MB
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::messages::{MessageType, MessagePayload, Heartbeat};
    use uuid::Uuid;
    use tokio::io::Cursor;

    fn create_test_message() -> Message {
        let payload = MessagePayload::Heartbeat(Heartbeat {
            client_id: Uuid::new_v4(),
            active_tunnels: 2,
            active_connections: 5,
            cpu_usage: 0.25,
            memory_usage: 1024 * 1024,
        });
        
        Message::new(MessageType::Heartbeat, payload)
    }

    #[test]
    fn test_encode_decode() {
        let codec = MessageCodec::default();
        let message = create_test_message();
        
        // エンコード
        let encoded = codec.encode(&message).unwrap();
        assert!(encoded.len() > 4); // 長さプレフィックス + JSON
        
        // デコード
        let decoded = codec.decode(&encoded).unwrap();
        assert_eq!(message.id, decoded.id);
        assert_eq!(message.version, decoded.version);
        assert_eq!(message.message_type, decoded.message_type);
    }

    #[test]
    fn test_message_too_large() {
        let codec = MessageCodec::new(100); // 小さなサイズ制限
        let message = create_test_message();
        
        let result = codec.encode(&message);
        assert!(matches!(result, Err(CodecError::MessageTooLarge { .. })));
    }

    #[test]
    fn test_invalid_data() {
        let codec = MessageCodec::default();
        
        // 短すぎるデータ
        let result = codec.decode(&[1, 2, 3]);
        assert!(matches!(result, Err(CodecError::InvalidLength { .. })));
        
        // 無効なJSON
        let mut invalid_data = vec![0u8; 4];
        invalid_data.extend_from_slice(b"invalid json");
        invalid_data[0..4].copy_from_slice(&(12u32).to_be_bytes());
        
        let result = codec.decode(&invalid_data);
        assert!(matches!(result, Err(CodecError::Decoding { .. })));
    }

    #[tokio::test]
    async fn test_async_read_write() {
        let codec = MessageCodec::default();
        let message = create_test_message();
        
        // エンコード
        let encoded = codec.encode(&message).unwrap();
        
        // 非同期読み取りテスト
        let mut cursor = Cursor::new(encoded.clone());
        let read_message = codec.read_message(&mut cursor).await.unwrap();
        
        assert_eq!(message.id, read_message.id);
        assert_eq!(message.version, read_message.version);
        assert_eq!(message.message_type, read_message.message_type);
        
        // 非同期書き込みテスト
        let mut buffer = Vec::new();
        codec.write_message(&mut buffer, &message).await.unwrap();
        
        assert_eq!(encoded, buffer);
    }

    #[test]
    fn test_extract_message_buffering() {
        let codec = MessageCodec::default();
        let message = create_test_message();
        let encoded = codec.encode(&message).unwrap();
        
        // 部分的なデータ
        let mut buffer = encoded[0..encoded.len() / 2].to_vec();
        let result = codec.extract_message(&mut buffer).unwrap();
        assert!(result.is_none()); // 不完全なメッセージ
        
        // 完全なデータ
        buffer.extend_from_slice(&encoded[encoded.len() / 2..]);
        let result = codec.extract_message(&mut buffer).unwrap();
        assert!(result.is_some()); // 完全なメッセージ
        assert!(buffer.is_empty()); // バッファから削除されている
        
        let extracted = result.unwrap();
        assert_eq!(message.id, extracted.id);
    }

    #[tokio::test]
    async fn test_connection_closed() {
        let codec = MessageCodec::default();
        let mut empty_cursor = Cursor::new(Vec::<u8>::new());
        
        let result = codec.read_message(&mut empty_cursor).await;
        assert!(matches!(result, Err(CodecError::ConnectionClosed)));
    }
}