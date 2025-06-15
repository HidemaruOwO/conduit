// 認証・認可システム
//
// トークンベース認証とEd25519署名による認証を提供します。
// セッション管理と権限管理の基盤を実装します。

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn, error};
use uuid::Uuid;

use super::crypto::{Ed25519Signature, verify_signature};
use super::keys::KeyManager;

/// 認証関連のエラー
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Authentication failed: {message}")]
    AuthenticationFailed { message: String },
    
    #[error("Authorization failed: {message}")]
    AuthorizationFailed { message: String },
    
    #[error("Token invalid: {message}")]
    InvalidToken { message: String },
    
    #[error("Token expired: {token_id}")]
    TokenExpired { token_id: String },
    
    #[error("Session invalid: {session_id}")]
    InvalidSession { session_id: String },
    
    #[error("Session expired: {session_id}")]
    SessionExpired { session_id: String },
    
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
    
    #[error("Permission denied: {action}")]
    PermissionDenied { action: String },
    
    #[error("Configuration error: {message}")]
    Configuration { message: String },
}

/// 認証の結果型
pub type AuthResult<T> = Result<T, AuthError>;

/// 認証トークン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    /// トークンID
    pub token_id: String,
    
    /// 発行日時
    pub issued_at: DateTime<Utc>,
    
    /// 有効期限
    pub expires_at: DateTime<Utc>,
    
    /// 発行者
    pub issuer: String,
    
    /// 対象
    pub subject: String,
    
    /// 権限
    pub permissions: Vec<Permission>,
    
    /// カスタムクレーム
    pub claims: HashMap<String, String>,
}

impl AuthToken {
    /// 新しいトークンを作成
    pub fn new(
        issuer: String,
        subject: String,
        permissions: Vec<Permission>,
        duration: Duration,
    ) -> Self {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::from_std(duration).unwrap_or_default();
        
        Self {
            token_id: Uuid::new_v4().to_string(),
            issued_at: now,
            expires_at,
            issuer,
            subject,
            permissions,
            claims: HashMap::new(),
        }
    }
    
    /// トークンが有効かチェック
    pub fn is_valid(&self) -> bool {
        Utc::now() < self.expires_at
    }
    
    /// 権限をチェック
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions.contains(permission)
    }
    
    /// カスタムクレームを追加
    pub fn add_claim(&mut self, key: String, value: String) {
        self.claims.insert(key, value);
    }
    
    /// カスタムクレームを取得
    pub fn get_claim(&self, key: &str) -> Option<&String> {
        self.claims.get(key)
    }
}

/// 権限
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// トンネル作成
    CreateTunnel,
    
    /// トンネル削除
    DeleteTunnel,
    
    /// トンネル一覧表示
    ListTunnels,
    
    /// 接続管理
    ManageConnections,
    
    /// システム監視
    SystemMonitoring,
    
    /// 設定管理
    ConfigManagement,
    
    /// 鍵管理
    KeyManagement,
    
    /// 管理者権限
    AdminAccess,
}

/// セッション情報
#[derive(Debug, Clone)]
pub struct Session {
    /// セッションID
    pub session_id: String,
    
    /// 認証トークン
    pub token: AuthToken,
    
    /// 最終アクセス時刻
    pub last_access: DateTime<Utc>,
    
    /// クライアント情報
    pub client_info: ClientInfo,
}

impl Session {
    /// 新しいセッションを作成
    pub fn new(token: AuthToken, client_info: ClientInfo) -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            token,
            last_access: Utc::now(),
            client_info,
        }
    }
    
    /// セッションが有効かチェック
    pub fn is_valid(&self, timeout: Duration) -> bool {
        let timeout_duration = chrono::Duration::from_std(timeout).unwrap_or_default();
        self.token.is_valid() && (Utc::now() - self.last_access) < timeout_duration
    }
    
    /// アクセス時刻を更新
    pub fn update_access(&mut self) {
        self.last_access = Utc::now();
    }
}

/// クライアント情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// クライアントID
    pub client_id: String,
    
    /// IPアドレス
    pub ip_address: String,
    
    /// ユーザーエージェント
    pub user_agent: Option<String>,
    
    /// 公開鍵
    pub public_key: Vec<u8>,
}

/// 認証リクエスト
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequest {
    /// クライアント情報
    pub client_info: ClientInfo,
    
    /// チャレンジデータ
    pub challenge: Vec<u8>,
    
    /// 署名
    pub signature: String,
    
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
}

/// 認証レスポンス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    /// 認証成功フラグ
    pub success: bool,
    
    /// セッションID
    pub session_id: Option<String>,
    
    /// 認証トークン
    pub token: Option<AuthToken>,
    
    /// エラーメッセージ
    pub error_message: Option<String>,
}

/// 認証管理システム
pub struct AuthManager {
    /// 鍵管理システム
    key_manager: KeyManager,
    
    /// アクティブなセッション
    sessions: HashMap<String, Session>,
    
    /// セッションタイムアウト
    session_timeout: Duration,
    
    /// トークン有効期限
    token_duration: Duration,
    
    /// 許可されたクライアント公開鍵
    authorized_clients: HashMap<String, Vec<u8>>,
}

impl AuthManager {
    /// 新しい認証管理システムを作成
    pub fn new(
        key_manager: KeyManager,
        session_timeout: Duration,
        token_duration: Duration,
    ) -> Self {
        Self {
            key_manager,
            sessions: HashMap::new(),
            session_timeout,
            token_duration,
            authorized_clients: HashMap::new(),
        }
    }
    
    /// クライアントを認証
    pub fn authenticate(&mut self, request: AuthRequest) -> AuthResult<AuthResponse> {
        // タイムスタンプの検証
        let now = Utc::now();
        let time_diff = (now - request.timestamp).num_seconds().abs();
        if time_diff > 300 { // 5分以内のタイムスタンプのみ許可
            return Ok(AuthResponse {
                success: false,
                session_id: None,
                token: None,
                error_message: Some("Request timestamp is too old".to_string()),
            });
        }
        
        // 署名の検証
        let signature = Ed25519Signature::from_base64(&request.signature)
            .map_err(|_| AuthError::SignatureVerificationFailed)?;
        
        let verify_data = self.create_verify_data(&request)?;
        let is_valid = verify_signature(&request.client_info.public_key, &verify_data, &signature)
            .map_err(|_| AuthError::SignatureVerificationFailed)?;
        
        if !is_valid {
            warn!("Signature verification failed for client: {}", request.client_info.client_id);
            return Ok(AuthResponse {
                success: false,
                session_id: None,
                token: None,
                error_message: Some("Signature verification failed".to_string()),
            });
        }
        
        // クライアント認可の確認
        if !self.is_client_authorized(&request.client_info) {
            warn!("Unauthorized client: {}", request.client_info.client_id);
            return Ok(AuthResponse {
                success: false,
                session_id: None,
                token: None,
                error_message: Some("Client not authorized".to_string()),
            });
        }
        
        // トークンとセッションを作成
        let permissions = self.get_client_permissions(&request.client_info);
        let token = AuthToken::new(
            "conduit-router".to_string(),
            request.client_info.client_id.clone(),
            permissions,
            self.token_duration,
        );
        
        let session = Session::new(token.clone(), request.client_info.clone());
        let session_id = session.session_id.clone();
        
        self.sessions.insert(session_id.clone(), session);
        
        info!("Client authenticated successfully: {}", request.client_info.client_id);
        
        Ok(AuthResponse {
            success: true,
            session_id: Some(session_id),
            token: Some(token),
            error_message: None,
        })
    }
    
    /// セッションを検証
    pub fn validate_session(&mut self, session_id: &str) -> AuthResult<bool> {
        let is_valid = if let Some(session) = self.sessions.get(session_id) {
            session.is_valid(self.session_timeout)
        } else {
            return Err(AuthError::InvalidSession {
                session_id: session_id.to_string()
            });
        };
        
        if !is_valid {
            self.sessions.remove(session_id);
            return Err(AuthError::SessionExpired {
                session_id: session_id.to_string()
            });
        }
        
        // セッションのアクセス時刻を更新
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.update_access();
        }
        
        Ok(true)
    }
    
    /// 権限をチェック
    pub fn check_permission(
        &mut self,
        session_id: &str,
        permission: &Permission,
    ) -> AuthResult<bool> {
        self.validate_session(session_id)?;
        
        if let Some(session) = self.sessions.get(session_id) {
            Ok(session.token.has_permission(permission))
        } else {
            Err(AuthError::InvalidSession {
                session_id: session_id.to_string()
            })
        }
    }
    
    /// セッションを終了
    pub fn logout(&mut self, session_id: &str) -> AuthResult<()> {
        self.sessions.remove(session_id);
        debug!("Session logged out: {}", session_id);
        Ok(())
    }
    
    /// 期限切れセッションをクリーンアップ
    pub fn cleanup_expired_sessions(&mut self) {
        let expired_sessions: Vec<String> = self.sessions
            .iter()
            .filter(|(_, session)| !session.is_valid(self.session_timeout))
            .map(|(id, _)| id.clone())
            .collect();
        
        for session_id in expired_sessions {
            self.sessions.remove(&session_id);
            debug!("Cleaned up expired session: {}", session_id);
        }
    }
    
    /// クライアントを認可リストに追加
    pub fn authorize_client(&mut self, client_id: String, public_key: Vec<u8>) {
        self.authorized_clients.insert(client_id.clone(), public_key);
        info!("Client authorized: {}", client_id);
    }
    
    /// クライアントを認可リストから削除
    pub fn revoke_client(&mut self, client_id: &str) {
        self.authorized_clients.remove(client_id);
        
        // 関連するセッションも削除
        let sessions_to_remove: Vec<String> = self.sessions
            .iter()
            .filter(|(_, session)| session.client_info.client_id == client_id)
            .map(|(id, _)| id.clone())
            .collect();
        
        for session_id in sessions_to_remove {
            self.sessions.remove(&session_id);
        }
        
        info!("Client revoked: {}", client_id);
    }
    
    /// チャレンジデータを生成
    pub fn generate_challenge(&self) -> Vec<u8> {
        use super::crypto::generate_random_bytes;
        generate_random_bytes(32)
    }
    
    /// 検証用データを作成
    fn create_verify_data(&self, request: &AuthRequest) -> AuthResult<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(&request.challenge);
        data.extend_from_slice(request.client_info.client_id.as_bytes());
        data.extend_from_slice(&request.client_info.public_key);
        data.extend_from_slice(&request.timestamp.timestamp().to_be_bytes());
        Ok(data)
    }
    
    /// クライアントが認可されているかチェック
    fn is_client_authorized(&self, client_info: &ClientInfo) -> bool {
        if let Some(authorized_key) = self.authorized_clients.get(&client_info.client_id) {
            authorized_key == &client_info.public_key
        } else {
            false
        }
    }
    
    /// クライアントの権限を取得
    fn get_client_permissions(&self, _client_info: &ClientInfo) -> Vec<Permission> {
        // 基本的な権限を付与
        vec![
            Permission::CreateTunnel,
            Permission::DeleteTunnel,
            Permission::ListTunnels,
            Permission::ManageConnections,
        ]
    }
    
    /// アクティブなセッション数を取得
    pub fn active_session_count(&self) -> usize {
        self.sessions.len()
    }
    
    /// セッション情報を取得
    pub fn get_session_info(&self, session_id: &str) -> Option<&Session> {
        self.sessions.get(session_id)
    }
    
    /// 全セッション情報を取得
    pub fn list_sessions(&self) -> Vec<&Session> {
        self.sessions.values().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;
    use crate::security::keys::{KeyManager, KeyRotationConfig};
    use crate::security::crypto::Ed25519KeyPair;

    fn create_test_auth_manager() -> AuthManager {
        let dir = tempdir().unwrap();
        let key_config = KeyRotationConfig::default();
        let key_manager = KeyManager::new(dir.path(), key_config).unwrap();
        
        AuthManager::new(
            key_manager,
            Duration::from_secs(3600), // 1時間
            Duration::from_secs(86400), // 24時間
        )
    }

    #[test]
    fn test_auth_token_creation() {
        let permissions = vec![Permission::CreateTunnel, Permission::ListTunnels];
        let token = AuthToken::new(
            "test-issuer".to_string(),
            "test-subject".to_string(),
            permissions.clone(),
            Duration::from_secs(3600),
        );
        
        assert!(token.is_valid());
        assert!(token.has_permission(&Permission::CreateTunnel));
        assert!(token.has_permission(&Permission::ListTunnels));
        assert!(!token.has_permission(&Permission::AdminAccess));
    }

    #[test]
    fn test_session_creation() {
        let permissions = vec![Permission::CreateTunnel];
        let token = AuthToken::new(
            "test-issuer".to_string(),
            "test-client".to_string(),
            permissions,
            Duration::from_secs(3600),
        );
        
        let client_info = ClientInfo {
            client_id: "test-client".to_string(),
            ip_address: "127.0.0.1".to_string(),
            user_agent: Some("test-agent".to_string()),
            public_key: vec![1, 2, 3, 4],
        };
        
        let session = Session::new(token, client_info);
        assert!(session.is_valid(Duration::from_secs(3600)));
    }

    #[test]
    fn test_client_authorization() {
        let mut auth_manager = create_test_auth_manager();
        let keypair = Ed25519KeyPair::generate().unwrap();
        
        let client_id = "test-client".to_string();
        let public_key = keypair.public_key_bytes().to_vec();
        
        // クライアントを認可
        auth_manager.authorize_client(client_id.clone(), public_key.clone());
        
        let client_info = ClientInfo {
            client_id: client_id.clone(),
            ip_address: "127.0.0.1".to_string(),
            user_agent: None,
            public_key: public_key.clone(),
        };
        
        assert!(auth_manager.is_client_authorized(&client_info));
        
        // 認可を取り消し
        auth_manager.revoke_client(&client_id);
        assert!(!auth_manager.is_client_authorized(&client_info));
    }

    #[test]
    fn test_challenge_generation() {
        let auth_manager = create_test_auth_manager();
        let challenge1 = auth_manager.generate_challenge();
        let challenge2 = auth_manager.generate_challenge();
        
        assert_eq!(challenge1.len(), 32);
        assert_eq!(challenge2.len(), 32);
        assert_ne!(challenge1, challenge2); // 異なるチャレンジが生成されることを確認
    }

    #[test]
    fn test_session_cleanup() {
        let mut auth_manager = create_test_auth_manager();
        
        // 期限切れのトークンでセッションを作成
        let mut token = AuthToken::new(
            "test-issuer".to_string(),
            "test-client".to_string(),
            vec![Permission::CreateTunnel],
            Duration::from_secs(1),
        );
        
        // 期限を過去に設定
        token.expires_at = Utc::now() - chrono::Duration::hours(1);
        
        let client_info = ClientInfo {
            client_id: "test-client".to_string(),
            ip_address: "127.0.0.1".to_string(),
            user_agent: None,
            public_key: vec![1, 2, 3, 4],
        };
        
        let session = Session::new(token, client_info);
        let session_id = session.session_id.clone();
        auth_manager.sessions.insert(session_id.clone(), session);
        
        assert_eq!(auth_manager.active_session_count(), 1);
        
        // クリーンアップを実行
        auth_manager.cleanup_expired_sessions();
        assert_eq!(auth_manager.active_session_count(), 0);
    }
}