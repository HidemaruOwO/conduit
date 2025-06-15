// 鍵管理システム
//
// Ed25519キーペアの生成・保存・読み込み・ローテーション機能を提供します。
// 30日間隔での鍵ローテーションと24時間グレースピリオド管理を実装します。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn, error};

use super::crypto::{Ed25519KeyPair, Ed25519Error};

/// 鍵管理関連のエラー
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("Key generation error: {message}")]
    Generation { message: String },
    
    #[error("Key loading error: {message}")]
    Loading { message: String },
    
    #[error("Key saving error: {message}")]
    Saving { message: String },
    
    #[error("Key rotation error: {message}")]
    Rotation { message: String },
    
    #[error("Key not found: {key_id}")]
    NotFound { key_id: String },
    
    #[error("Key expired: {key_id}")]
    Expired { key_id: String },
    
    #[error("Configuration error: {message}")]
    Configuration { message: String },
    
    #[error("File operation error: {message}")]
    FileOperation { message: String },
    
    #[error("Crypto error: {0}")]
    Crypto(#[from] Ed25519Error),
}

/// 鍵管理の結果型
pub type KeyResult<T> = Result<T, KeyError>;

/// 鍵ローテーション設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationConfig {
    /// 鍵ローテーション間隔（日）
    pub rotation_interval_days: u32,
    
    /// グレースピリオド（時間）
    pub grace_period_hours: u32,
    
    /// 自動ローテーションを有効にするか
    pub auto_rotation_enabled: bool,
    
    /// 最大保持する古い鍵の数
    pub max_old_keys: u32,
}

impl Default for KeyRotationConfig {
    fn default() -> Self {
        Self {
            rotation_interval_days: 30,
            grace_period_hours: 24,
            auto_rotation_enabled: true,
            max_old_keys: 5,
        }
    }
}

/// 鍵のメタデータ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMetadata {
    /// 鍵ID
    pub key_id: String,
    
    /// 作成日時
    pub created_at: DateTime<Utc>,
    
    /// 有効期限
    pub expires_at: DateTime<Utc>,
    
    /// アクティブかどうか
    pub is_active: bool,
    
    /// 鍵の用途
    pub purpose: KeyPurpose,
    
    /// 鍵のバージョン
    pub version: u32,
}

/// 鍵の用途
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum KeyPurpose {
    /// クライアント認証用
    ClientAuth,
    
    /// サーバー認証用
    ServerAuth,
    
    /// 汎用署名用
    Signing,
}

/// 鍵エントリ
#[derive(Debug, Clone)]
pub struct KeyEntry {
    /// メタデータ
    pub metadata: KeyMetadata,
    
    /// キーペア
    pub keypair: Ed25519KeyPair,
}

/// 鍵管理システム
pub struct KeyManager {
    /// 鍵ディレクトリ
    key_dir: PathBuf,
    
    /// ローテーション設定
    rotation_config: KeyRotationConfig,
    
    /// 読み込み済みの鍵
    keys: HashMap<String, KeyEntry>,
    
    /// アクティブな鍵ID
    active_key_id: Option<String>,
}

impl KeyManager {
    /// 新しい鍵管理システムを作成
    pub fn new<P: AsRef<Path>>(key_dir: P, rotation_config: KeyRotationConfig) -> KeyResult<Self> {
        let key_dir = key_dir.as_ref().to_path_buf();
        
        // ディレクトリが存在しない場合は作成
        if !key_dir.exists() {
            fs::create_dir_all(&key_dir)
                .map_err(|e| KeyError::FileOperation {
                    message: format!("Failed to create key directory: {}", e)
                })?;
        }
        
        let mut manager = Self {
            key_dir,
            rotation_config,
            keys: HashMap::new(),
            active_key_id: None,
        };
        
        // 既存の鍵を読み込み
        manager.load_existing_keys()?;
        
        Ok(manager)
    }
    
    /// 新しい鍵を生成
    pub fn generate_key(&mut self, purpose: KeyPurpose) -> KeyResult<String> {
        let key_id = generate_key_id();
        let keypair = Ed25519KeyPair::generate()
            .map_err(|e| KeyError::Generation {
                message: format!("Failed to generate keypair: {}", e)
            })?;
        
        let now = Utc::now();
        let expires_at = now + chrono::Duration::days(self.rotation_config.rotation_interval_days as i64);
        
        let metadata = KeyMetadata {
            key_id: key_id.clone(),
            created_at: now,
            expires_at,
            is_active: true,
            purpose,
            version: 1,
        };
        
        let entry = KeyEntry {
            metadata: metadata.clone(),
            keypair,
        };
        
        // 既存のアクティブ鍵を非アクティブにする
        self.deactivate_all_keys();
        
        // 新しい鍵を保存
        self.save_key(&entry)?;
        
        // メモリに追加
        self.keys.insert(key_id.clone(), entry);
        self.active_key_id = Some(key_id.clone());
        
        info!("Generated new key: {}", key_id);
        
        Ok(key_id)
    }
    
    /// 鍵を読み込み
    pub fn load_key(&mut self, key_id: &str) -> KeyResult<&KeyEntry> {
        if !self.keys.contains_key(key_id) {
            self.load_key_from_disk(key_id)?;
        }
        
        self.keys.get(key_id)
            .ok_or_else(|| KeyError::NotFound { key_id: key_id.to_string() })
    }
    
    /// アクティブな鍵を取得
    pub fn get_active_key(&self) -> KeyResult<&KeyEntry> {
        let active_id = self.active_key_id.as_ref()
            .ok_or_else(|| KeyError::NotFound { key_id: "active".to_string() })?;
        
        self.keys.get(active_id)
            .ok_or_else(|| KeyError::NotFound { key_id: active_id.clone() })
    }
    
    /// 鍵の有効性を確認
    pub fn is_key_valid(&self, key_id: &str) -> bool {
        if let Some(entry) = self.keys.get(key_id) {
            let now = Utc::now();
            let grace_period = chrono::Duration::hours(self.rotation_config.grace_period_hours as i64);
            
            entry.metadata.is_active || now <= entry.metadata.expires_at + grace_period
        } else {
            false
        }
    }
    
    /// 鍵ローテーションが必要かチェック
    pub fn needs_rotation(&self) -> bool {
        if !self.rotation_config.auto_rotation_enabled {
            return false;
        }
        
        if let Ok(active_key) = self.get_active_key() {
            let now = Utc::now();
            let rotation_threshold = active_key.metadata.created_at + 
                chrono::Duration::days(self.rotation_config.rotation_interval_days as i64);
            
            now >= rotation_threshold
        } else {
            true // アクティブ鍵がない場合はローテーションが必要
        }
    }
    
    /// 鍵ローテーションを実行
    pub fn rotate_keys(&mut self, purpose: KeyPurpose) -> KeyResult<String> {
        info!("Starting key rotation");
        
        // 新しい鍵を生成
        let new_key_id = self.generate_key(purpose)?;
        
        // 古い鍵をクリーンアップ
        self.cleanup_old_keys()?;
        
        info!("Key rotation completed, new active key: {}", new_key_id);
        
        Ok(new_key_id)
    }
    
    /// 古い鍵をクリーンアップ
    fn cleanup_old_keys(&mut self) -> KeyResult<()> {
        let mut old_keys: Vec<_> = self.keys.values()
            .filter(|entry| !entry.metadata.is_active)
            .map(|entry| entry.metadata.key_id.clone())
            .collect();
        
        // キーIDでソート（作成日時情報を保持するため、再度情報を取得）
        old_keys.sort_by(|a, b| {
            let a_time = self.keys.get(a).map(|entry| entry.metadata.created_at);
            let b_time = self.keys.get(b).map(|entry| entry.metadata.created_at);
            b_time.cmp(&a_time) // 新しい順
        });
        
        // 設定された最大数を超える古い鍵を削除
        if old_keys.len() > self.rotation_config.max_old_keys as usize {
            let keys_to_remove = &old_keys[self.rotation_config.max_old_keys as usize..];
            
            for key_id in keys_to_remove {
                self.delete_key(key_id)?;
                debug!("Cleaned up old key: {}", key_id);
            }
        }
        
        Ok(())
    }
    
    /// 鍵をディスクに保存
    fn save_key(&self, entry: &KeyEntry) -> KeyResult<()> {
        let key_id = &entry.metadata.key_id;
        
        // メタデータファイルのパス
        let metadata_path = self.key_dir.join(format!("{}.metadata.json", key_id));
        let metadata_json = serde_json::to_string_pretty(&entry.metadata)
            .map_err(|e| KeyError::Saving {
                message: format!("Failed to serialize metadata: {}", e)
            })?;
        
        fs::write(&metadata_path, metadata_json)
            .map_err(|e| KeyError::Saving {
                message: format!("Failed to write metadata file: {}", e)
            })?;
        
        // 秘密鍵ファイルのパス
        let secret_key_path = self.key_dir.join(format!("{}.key", key_id));
        entry.keypair.save_secret_key(&secret_key_path)
            .map_err(|e| KeyError::Saving {
                message: format!("Failed to save secret key: {}", e)
            })?;
        
        // 公開鍵ファイルのパス
        let public_key_path = self.key_dir.join(format!("{}.pub", key_id));
        entry.keypair.save_public_key(&public_key_path)
            .map_err(|e| KeyError::Saving {
                message: format!("Failed to save public key: {}", e)
            })?;
        
        debug!("Saved key to disk: {}", key_id);
        
        Ok(())
    }
    
    /// ディスクから鍵を読み込み
    fn load_key_from_disk(&mut self, key_id: &str) -> KeyResult<()> {
        // メタデータファイルのパス
        let metadata_path = self.key_dir.join(format!("{}.metadata.json", key_id));
        let metadata_content = fs::read_to_string(&metadata_path)
            .map_err(|e| KeyError::Loading {
                message: format!("Failed to read metadata file: {}", e)
            })?;
        
        let metadata: KeyMetadata = serde_json::from_str(&metadata_content)
            .map_err(|e| KeyError::Loading {
                message: format!("Failed to parse metadata: {}", e)
            })?;
        
        // 秘密鍵ファイルのパス
        let secret_key_path = self.key_dir.join(format!("{}.key", key_id));
        let keypair = Ed25519KeyPair::from_file(&secret_key_path)
            .map_err(|e| KeyError::Loading {
                message: format!("Failed to load keypair: {}", e)
            })?;
        
        let entry = KeyEntry {
            metadata,
            keypair,
        };
        
        self.keys.insert(key_id.to_string(), entry);
        
        debug!("Loaded key from disk: {}", key_id);
        
        Ok(())
    }
    
    /// 既存の鍵をすべて読み込み
    fn load_existing_keys(&mut self) -> KeyResult<()> {
        let entries = fs::read_dir(&self.key_dir)
            .map_err(|e| KeyError::FileOperation {
                message: format!("Failed to read key directory: {}", e)
            })?;
        
        let mut active_key_id = None;
        let mut latest_active_time = None;
        
        for entry in entries {
            let entry = entry.map_err(|e| KeyError::FileOperation {
                message: format!("Failed to read directory entry: {}", e)
            })?;
            
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.ends_with(".metadata.json") {
                    let key_id = filename.replace(".metadata.json", "");
                    
                    if let Err(e) = self.load_key_from_disk(&key_id) {
                        warn!("Failed to load key {}: {}", key_id, e);
                        continue;
                    }
                    
                    // アクティブな鍵を見つける
                    if let Some(entry) = self.keys.get(&key_id) {
                        if entry.metadata.is_active {
                            if latest_active_time.is_none() || 
                               entry.metadata.created_at > latest_active_time.unwrap() {
                                active_key_id = Some(key_id.clone());
                                latest_active_time = Some(entry.metadata.created_at);
                            }
                        }
                    }
                }
            }
        }
        
        self.active_key_id = active_key_id;
        
        info!("Loaded {} keys from disk", self.keys.len());
        if let Some(active_id) = &self.active_key_id {
            info!("Active key: {}", active_id);
        }
        
        Ok(())
    }
    
    /// すべての鍵を非アクティブにする
    fn deactivate_all_keys(&mut self) {
        for entry in self.keys.values_mut() {
            entry.metadata.is_active = false;
        }
    }
    
    /// 鍵を削除
    fn delete_key(&mut self, key_id: &str) -> KeyResult<()> {
        // ファイルを削除
        let metadata_path = self.key_dir.join(format!("{}.metadata.json", key_id));
        let secret_key_path = self.key_dir.join(format!("{}.key", key_id));
        let public_key_path = self.key_dir.join(format!("{}.pub", key_id));
        
        for path in [metadata_path, secret_key_path, public_key_path] {
            if path.exists() {
                fs::remove_file(&path)
                    .map_err(|e| KeyError::FileOperation {
                        message: format!("Failed to delete file {:?}: {}", path, e)
                    })?;
            }
        }
        
        // メモリから削除
        self.keys.remove(key_id);
        
        if self.active_key_id.as_ref().map(|id| id.as_str()) == Some(key_id) {
            self.active_key_id = None;
        }
        
        Ok(())
    }
    
    /// 鍵の一覧を取得
    pub fn list_keys(&self) -> Vec<&KeyMetadata> {
        self.keys.values().map(|entry| &entry.metadata).collect()
    }
}

/// 鍵IDを生成
fn generate_key_id() -> String {
    use uuid::Uuid;
    format!("key_{}", Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_key_manager_creation() {
        let dir = tempdir().unwrap();
        let config = KeyRotationConfig::default();
        
        let manager = KeyManager::new(dir.path(), config).unwrap();
        assert!(manager.keys.is_empty());
        assert!(manager.active_key_id.is_none());
    }

    #[test]
    fn test_key_generation() {
        let dir = tempdir().unwrap();
        let config = KeyRotationConfig::default();
        let mut manager = KeyManager::new(dir.path(), config).unwrap();
        
        let key_id = manager.generate_key(KeyPurpose::ClientAuth).unwrap();
        assert!(manager.keys.contains_key(&key_id));
        assert_eq!(manager.active_key_id.as_ref(), Some(&key_id));
        
        let active_key = manager.get_active_key().unwrap();
        assert_eq!(active_key.metadata.key_id, key_id);
        assert!(active_key.metadata.is_active);
    }

    #[test]
    fn test_key_persistence() {
        let dir = tempdir().unwrap();
        let config = KeyRotationConfig::default();
        
        let key_id = {
            let mut manager = KeyManager::new(dir.path(), config.clone()).unwrap();
            manager.generate_key(KeyPurpose::ServerAuth).unwrap()
        };
        
        // 新しいマネージャーで鍵が読み込まれることを確認
        let manager = KeyManager::new(dir.path(), config).unwrap();
        assert!(manager.keys.contains_key(&key_id));
        assert_eq!(manager.active_key_id.as_ref(), Some(&key_id));
    }

    #[test]
    fn test_key_rotation() {
        let dir = tempdir().unwrap();
        let mut config = KeyRotationConfig::default();
        config.rotation_interval_days = 0; // 即座にローテーションが必要になるように設定
        
        let mut manager = KeyManager::new(dir.path(), config).unwrap();
        
        let old_key_id = manager.generate_key(KeyPurpose::ClientAuth).unwrap();
        assert!(manager.needs_rotation());
        
        let new_key_id = manager.rotate_keys(KeyPurpose::ClientAuth).unwrap();
        assert_ne!(old_key_id, new_key_id);
        assert_eq!(manager.active_key_id.as_ref(), Some(&new_key_id));
        
        // 古い鍵はまだ存在するが非アクティブ
        assert!(manager.keys.contains_key(&old_key_id));
        assert!(!manager.keys[&old_key_id].metadata.is_active);
    }

    #[test]
    fn test_key_validity() {
        let dir = tempdir().unwrap();
        let config = KeyRotationConfig::default();
        let mut manager = KeyManager::new(dir.path(), config).unwrap();
        
        let key_id = manager.generate_key(KeyPurpose::Signing).unwrap();
        assert!(manager.is_key_valid(&key_id));
        
        // 非アクティブにしても、グレースピリオド内なら有効
        manager.keys.get_mut(&key_id).unwrap().metadata.is_active = false;
        assert!(manager.is_key_valid(&key_id));
    }
}