// プロトコルハンドラー
//
// Client-Router間の非同期通信フローを管理します：
// - 非同期メッセージ処理
// - エラーハンドリングとリトライ機能
// - セキュリティ基盤との統合
// - タイムアウト管理

use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{timeout, sleep};
use tokio_rustls::{TlsConnector, TlsStream, client::TlsStream as ClientTlsStream};
use uuid::Uuid;
use dashmap::DashMap;
use tracing::{debug, info, warn, error, instrument};

use crate::protocol::{
    Message, MessageType, MessagePayload, ProtocolError,
    MessageCodec, CodecError, ProtocolResult, ProtocolModuleError,
};
use crate::security::{TlsClientConfig, AuthManager, SecurityResult};
use crate::common::error::Result;

#[derive(Debug, Clone)]
pub struct ProtocolHandlerConfig {
    pub connect_timeout_seconds: u64,
    
    /// メッセージタイムアウト（秒）
    pub message_timeout_seconds: u64,
    
    /// ハートビート間隔（秒）
    pub heartbeat_interval_seconds: u64,
    
    /// 最大再試行回数
    pub max_retries: usize,
    
    /// 再試行間隔（ミリ秒）
    pub retry_delay_ms: u64,
    
    /// 最大メッセージサイズ（バイト）
    pub max_message_size: usize,
    
    /// 接続キープアライブ有効
    pub keepalive_enabled: bool,
}

impl Default for ProtocolHandlerConfig {
    fn default() -> Self {
        Self {
            connect_timeout_seconds: 30,
            message_timeout_seconds: 30,
            heartbeat_interval_seconds: 60,
            max_retries: 3,
            retry_delay_ms: 1000,
            max_message_size: 1024 * 1024, // 1MB
            keepalive_enabled: true,
        }
    }
}

/// 接続状態
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Authenticated,
    Error(String),
}

/// メッセージハンドラー特性
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    async fn handle_message(&self, message: Message) -> ProtocolResult<Option<Message>>;
}

/// プロトコルハンドラー
pub struct ProtocolHandler {
    config: ProtocolHandlerConfig,
    codec: MessageCodec,
    tls_config: TlsClientConfig,
    auth_manager: Arc<AuthManager>,
    connection_state: Arc<RwLock<ConnectionState>>,
    pending_requests: Arc<DashMap<Uuid, mpsc::Sender<Message>>>,
    message_handler: Option<Arc<dyn MessageHandler>>,
}

impl ProtocolHandler {
    /// 新しいプロトコルハンドラーを作成
    pub fn new(
        config: ProtocolHandlerConfig,
        tls_config: TlsClientConfig,
        auth_manager: Arc<AuthManager>,
    ) -> Self {
        let codec = MessageCodec::new(config.max_message_size as u32);
        
        Self {
            config,
            codec,
            tls_config,
            auth_manager,
            connection_state: Arc::new(RwLock::new(ConnectionState::Disconnected)),
            pending_requests: Arc::new(DashMap::new()),
            message_handler: None,
        }
    }
    
    /// メッセージハンドラーを設定
    pub fn set_message_handler(&mut self, handler: Arc<dyn MessageHandler>) {
        self.message_handler = Some(handler);
    }
    
    /// 接続状態を取得
    pub async fn connection_state(&self) -> ConnectionState {
        self.connection_state.read().await.clone()
    }
    
    /// Routerに接続
    #[instrument(skip(self))]
    pub async fn connect(&self, router_addr: &str) -> ProtocolResult<ClientTlsStream<TcpStream>> {
        info!("Connecting to router: {}", router_addr);
        
        // 接続状態を更新
        *self.connection_state.write().await = ConnectionState::Connecting;
        
        let mut last_error = None;
        
        for attempt in 1..=self.config.max_retries {
            debug!("Connection attempt {} of {}", attempt, self.config.max_retries);
            
            match self.try_connect(router_addr).await {
                Ok(stream) => {
                    info!("Successfully connected to router");
                    *self.connection_state.write().await = ConnectionState::Connected;
                    return Ok(stream);
                }
                Err(e) => {
                    warn!("Connection attempt {} failed: {}", attempt, e);
                    last_error = Some(e);
                    
                    if attempt < self.config.max_retries {
                        let delay = Duration::from_millis(self.config.retry_delay_ms * attempt as u64);
                        debug!("Waiting {:?} before retry", delay);
                        sleep(delay).await;
                    }
                }
            }
        }
        
        let error_msg = format!("Failed to connect after {} attempts", self.config.max_retries);
        error!("{}", error_msg);
        *self.connection_state.write().await = ConnectionState::Error(error_msg.clone());
        
        Err(last_error.unwrap_or_else(|| {
            ProtocolModuleError::Handler {
                message: error_msg,
            }
        }))
    }
    
    /// 単一の接続試行
    async fn try_connect(&self, router_addr: &str) -> ProtocolResult<ClientTlsStream<TcpStream>> {
        // TCP接続
        let tcp_stream = timeout(
            Duration::from_secs(self.config.connect_timeout_seconds),
            TcpStream::connect(router_addr)
        ).await
        .map_err(|_| ProtocolModuleError::Handler {
            message: "Connection timeout".to_string(),
        })?
        .map_err(|e| ProtocolModuleError::Handler {
            message: format!("TCP connection failed: {}", e),
        })?;
        
        // TLS接続
        // 簡略化: TlsConnectorの直接作成
        let tls_config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth();
        let connector = TlsConnector::from(std::sync::Arc::new(tls_config));
        let domain = self.extract_domain(router_addr)?;
        
        let tls_stream = connector
            .connect(domain.try_into().unwrap(), tcp_stream)
            .await
            .map_err(|e| ProtocolModuleError::Handler {
                message: format!("TLS connection failed: {}", e),
            })?;
        
        Ok(tls_stream)
    }
    
    /// ドメイン名を抽出
    fn extract_domain<'a>(&self, addr: &'a str) -> ProtocolResult<&'a str> {
        let domain = addr.split(':').next().unwrap_or(addr);
        if domain.is_empty() {
            return Err(ProtocolModuleError::Handler {
                message: "Invalid address format".to_string(),
            });
        }
        Ok(domain)
    }
    
    /// メッセージを送信し、レスポンスを待機
    #[instrument(skip(self, stream, message))]
    pub async fn send_message(
        &self,
        stream: &mut ClientTlsStream<TcpStream>,
        message: Message,
    ) -> ProtocolResult<Message> {
        let message_id = message.id;
        debug!("Sending message: {} (type: {:?})", message_id, message.message_type);
        
        // レスポンス受信用チャンネル
        let (tx, mut rx) = mpsc::channel(1);
        self.pending_requests.insert(message_id, tx);
        
        // メッセージ送信
        self.codec.write_message(stream, &message).await
            .map_err(|e| ProtocolModuleError::Handler {
                message: format!("Failed to send message: {}", e),
            })?;
        
        // レスポンス待機（タイムアウト付き）
        let response = timeout(
            Duration::from_secs(self.config.message_timeout_seconds),
            rx.recv()
        ).await
        .map_err(|_| ProtocolModuleError::Handler {
            message: "Message timeout".to_string(),
        })?
        .ok_or_else(|| ProtocolModuleError::Handler {
            message: "Response channel closed".to_string(),
        })?;
        
        // リクエストを削除
        self.pending_requests.remove(&message_id);
        
        debug!("Received response for message: {}", message_id);
        Ok(response)
    }
    
    /// 非同期でメッセージを送信（レスポンス待機なし）
    #[instrument(skip(self, stream, message))]
    pub async fn send_message_async(
        &self,
        stream: &mut ClientTlsStream<TcpStream>,
        message: Message,
    ) -> ProtocolResult<()> {
        debug!("Sending async message: {} (type: {:?})", message.id, message.message_type);
        
        self.codec.write_message(stream, &message).await
            .map_err(|e| ProtocolModuleError::Handler {
                message: format!("Failed to send async message: {}", e),
            })?;
        
        Ok(())
    }
    
    /// メッセージ受信ループを開始
    #[instrument(skip(self, stream))]
    pub async fn start_message_loop(
        &self,
        mut stream: ClientTlsStream<TcpStream>,
    ) -> ProtocolResult<()> {
        info!("Starting message receive loop");
        
        loop {
            match self.codec.read_message(&mut stream).await {
                Ok(message) => {
                    debug!("Received message: {} (type: {:?})", message.id, message.message_type);
                    
                    if let Err(e) = self.handle_received_message(message).await {
                        warn!("Failed to handle received message: {}", e);
                    }
                }
                Err(CodecError::ConnectionClosed) => {
                    info!("Connection closed by remote");
                    *self.connection_state.write().await = ConnectionState::Disconnected;
                    break;
                }
                Err(e) => {
                    error!("Failed to read message: {}", e);
                    *self.connection_state.write().await = ConnectionState::Error(e.to_string());
                    return Err(ProtocolModuleError::Handler {
                        message: format!("Message loop error: {}", e),
                    });
                }
            }
        }
        
        Ok(())
    }
    
    /// 受信したメッセージを処理
    async fn handle_received_message(&self, message: Message) -> ProtocolResult<()> {
        // レスポンスメッセージの場合、対応するリクエストに送信
        if self.is_response_message(&message.message_type) {
            if let Some((_, tx)) = self.pending_requests.remove(&message.id) {
                if tx.send(message).await.is_err() {
                    warn!("Failed to send response to waiting request");
                }
                return Ok(());
            }
        }
        
        // カスタムハンドラーが設定されている場合、処理を委譲
        if let Some(handler) = &self.message_handler {
            match handler.handle_message(message).await {
                Ok(Some(response)) => {
                    // レスポンスがある場合は送信（実際の実装では送信手段が必要）
                    debug!("Handler returned response: {}", response.id);
                }
                Ok(None) => {
                    debug!("Handler processed message without response");
                }
                Err(e) => {
                    warn!("Message handler error: {}", e);
                }
            }
        }
        
        Ok(())
    }
    
    /// レスポンスメッセージかどうかを判定
    fn is_response_message(&self, message_type: &MessageType) -> bool {
        matches!(
            message_type,
            MessageType::ClientRegisterResponse
                | MessageType::TunnelCreateResponse
                | MessageType::TunnelDataResponse
                | MessageType::HeartbeatResponse
        )
    }
    
    /// ハートビートループを開始
    #[instrument(skip(self, stream))]
    pub async fn start_heartbeat_loop(
        &self,
        stream: Arc<Mutex<ClientTlsStream<TcpStream>>>,
        client_id: Uuid,
    ) -> ProtocolResult<()> {
        info!("Starting heartbeat loop");
        
        let mut interval = tokio::time::interval(
            Duration::from_secs(self.config.heartbeat_interval_seconds)
        );
        
        loop {
            interval.tick().await;
            
            let state = self.connection_state().await;
            if !matches!(state, ConnectionState::Connected | ConnectionState::Authenticated) {
                debug!("Heartbeat stopped due to connection state: {:?}", state);
                break;
            }
            
            // ハートビートメッセージを作成
            let heartbeat = MessagePayload::Heartbeat(crate::protocol::messages::Heartbeat {
                client_id,
                active_tunnels: 0, // TODO: 実際の値を取得
                active_connections: 0, // TODO: 実際の値を取得
                cpu_usage: 0.0, // TODO: 実際の値を取得
                memory_usage: 0, // TODO: 実際の値を取得
            });
            
            let message = Message::new(MessageType::Heartbeat, heartbeat);
            
            // ハートビート送信
            let mut stream_guard = stream.lock().await;
            if let Err(e) = self.send_message_async(&mut *stream_guard, message).await {
                error!("Failed to send heartbeat: {}", e);
                break;
            }
            
            debug!("Heartbeat sent");
        }
        
        Ok(())
    }
    
    /// 認証を実行
    #[instrument(skip(self, stream))]
    pub async fn authenticate(
        &self,
        stream: &mut ClientTlsStream<TcpStream>,
        client_id: Uuid,
        client_name: String,
    ) -> ProtocolResult<()> {
        info!("Authenticating client: {} ({})", client_name, client_id);
        
        // 認証メッセージを作成
        // 認証リクエストを作成（簡略化）
        // 実際の実装では、より適切な認証フローが必要
        // TODO: 適切なAuthRequestを作成し、認証を実行
        
        let register = MessagePayload::ClientRegister(crate::protocol::messages::ClientRegister {
            client_id,
            client_name,
            public_key: "TODO".to_string(), // TODO: 実際の公開鍵を設定
            signature: "TODO".to_string(), // TODO: 実際の署名を設定
            client_version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: vec!["tcp".to_string(), "heartbeat".to_string()],
        });
        
        let message = Message::new(MessageType::ClientRegister, register);
        
        // 認証メッセージを送信し、レスポンスを待機
        let response = self.send_message(stream, message).await?;
        
        // レスポンスを処理
        match response.payload {
            MessagePayload::ClientRegisterResponse(ref resp) => {
                if resp.success {
                    info!("Authentication successful");
                    *self.connection_state.write().await = ConnectionState::Authenticated;
                    Ok(())
                } else {
                    let error_msg = resp.error.as_deref().unwrap_or("Unknown auth error");
                    error!("Authentication failed: {}", error_msg);
                    Err(ProtocolModuleError::Handler {
                        message: format!("Authentication failed: {}", error_msg),
                    })
                }
            }
            _ => {
                error!("Unexpected response type for authentication");
                Err(ProtocolModuleError::Handler {
                    message: "Unexpected response type for authentication".to_string(),
                })
            }
        }
    }
    
    /// 切断処理
    #[instrument(skip(self))]
    pub async fn disconnect(&self) -> ProtocolResult<()> {
        info!("Disconnecting from router");
        
        // 接続状態を更新
        *self.connection_state.write().await = ConnectionState::Disconnected;
        
        // 待機中のリクエストをクリア
        self.pending_requests.clear();
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{TlsConfig, AuthManager};
    use std::sync::Arc;

    fn create_test_handler() -> ProtocolHandler {
        let config = ProtocolHandlerConfig::default();
        let tls_config = TlsClientConfig::new(TlsConfig::default()).unwrap();
        let auth_manager = Arc::new(AuthManager::new());
        
        ProtocolHandler::new(config, tls_config, auth_manager)
    }

    #[test]
    fn test_handler_creation() {
        let handler = create_test_handler();
        assert!(matches!(
            tokio::runtime::Runtime::new().unwrap().block_on(handler.connection_state()),
            ConnectionState::Disconnected
        ));
    }

    #[test]
    fn test_domain_extraction() {
        let handler = create_test_handler();
        
        assert_eq!(handler.extract_domain("localhost:8080").unwrap(), "localhost");
        assert_eq!(handler.extract_domain("127.0.0.1:9999").unwrap(), "127.0.0.1");
        assert_eq!(handler.extract_domain("example.com").unwrap(), "example.com");
    }

    #[test]
    fn test_response_message_detection() {
        let handler = create_test_handler();
        
        assert!(handler.is_response_message(&MessageType::ClientRegisterResponse));
        assert!(handler.is_response_message(&MessageType::TunnelCreateResponse));
        assert!(!handler.is_response_message(&MessageType::ClientRegister));
        assert!(!handler.is_response_message(&MessageType::Heartbeat));
    }
}