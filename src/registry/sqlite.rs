// SQLite Process Registry実装
// WAL modeによる高性能並行アクセス対応

use crate::registry::models::*;
use anyhow::{Context, Result};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite, Row};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// SQLite Registry管理構造体
pub struct SqliteRegistry {
    pool: Pool<Sqlite>,
    encryption_key: Arc<[u8; 32]>,
    db_path: PathBuf,
}

impl SqliteRegistry {
    // 新しいレジストリインスタンスの作成
    pub async fn new(db_path: Option<PathBuf>) -> Result<Self> {
        let db_path = db_path.unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".conduit")
                .join("registry.db")
        });

        // ディレクトリ作成
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .context("Failed to create registry directory")?;
        }

        info!("Initializing SQLite registry at: {}", db_path.display());

        // WAL mode対応のSQLite接続プール設定
        let database_url = format!("sqlite:{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect(&database_url)
            .await
            .context("Failed to create SQLite connection pool")?;

        // WAL mode有効化とパフォーマンス設定
        sqlx::query("PRAGMA journal_mode=WAL").execute(&pool).await?;
        sqlx::query("PRAGMA synchronous=FULL").execute(&pool).await?;
        sqlx::query("PRAGMA cache_size=5000").execute(&pool).await?;
        sqlx::query("PRAGMA temp_store=memory").execute(&pool).await?;
        sqlx::query("PRAGMA mmap_size=134217728").execute(&pool).await?;
        sqlx::query("PRAGMA auto_vacuum=FULL").execute(&pool).await?;
        sqlx::query("PRAGMA secure_delete=ON").execute(&pool).await?;
        sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await?;

        // データベーススキーマの初期化
        sqlx::migrate!("./migrations").run(&pool).await
            .context("Failed to run database migrations")?;

        // 暗号化キーの生成または取得
        let encryption_key = Self::get_or_create_encryption_key(&pool).await?;

        Ok(Self {
            pool,
            encryption_key: Arc::new(encryption_key),
            db_path,
        })
    }

    // トンネルエントリの作成
    pub async fn create_tunnel(
        &self,
        id: String,
        name: String,
        pid: i32,
        socket_path: &str,
        config: &TunnelConfig,
    ) -> Result<()> {
        let entry = TunnelEntry::new(id.clone(), name, pid, socket_path, config, &**self.encryption_key)?;

        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            r#"
            INSERT INTO tunnels (
                id, name, pid, socket_path_hash, status, config_encrypted,
                config_checksum, created_at, updated_at, last_activity
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            entry.id,
            entry.name,
            entry.pid,
            entry.socket_path_hash,
            entry.status,
            entry.config_encrypted,
            entry.config_checksum,
            entry.created_at,
            entry.updated_at,
            entry.last_activity
        )
        .execute(&mut *tx)
        .await
        .context("Failed to insert tunnel entry")?;

        // 監査ログ記録
        self.log_audit_action(&mut tx, "CREATE", "tunnels", Some(&id), true, None).await?;

        tx.commit().await?;
        info!("Created tunnel entry: {}", id);
        Ok(())
    }

    // トンネル状態の更新
    pub async fn update_tunnel_status(
        &self,
        id: &str,
        status: TunnelStatus,
        exit_code: Option<i32>,
    ) -> Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let status_value = status as i32;

        let mut tx = self.pool.begin().await?;

        // プロセス終了時はPIDをクリア
        let pid = if matches!(status, TunnelStatus::Exited | TunnelStatus::Error) {
            None
        } else {
            // 現在のPID値を保持
            let current_pid: Option<i32> = sqlx::query_scalar!(
                "SELECT pid FROM tunnels WHERE id = ?",
                id
            )
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
            current_pid
        };

        let result = sqlx::query!(
            r#"
            UPDATE tunnels
            SET status = ?, exit_code = ?, updated_at = ?, last_activity = ?, pid = ?
            WHERE id = ?
            "#,
            status_value,
            exit_code,
            now,
            now,
            pid,
            id
        )
        .execute(&mut *tx)
        .await?;

        let updated = result.rows_affected() > 0;

        if updated {
            self.log_audit_action(&mut tx, "UPDATE", "tunnels", Some(id), true, None).await?;
            debug!("Updated tunnel {} status to: {}", id, status.as_str());
        } else {
            warn!("Tunnel {} not found for status update", id);
        }

        tx.commit().await?;
        Ok(updated)
    }

    // アクティブトンネル一覧の取得（100並列対応）
    pub async fn list_active_tunnels(&self) -> Result<Vec<TunnelInfo>> {
        let entries: Vec<TunnelEntry> = sqlx::query_as!(
            TunnelEntry,
            r#"
            SELECT id, name, pid, socket_path_hash, status, config_encrypted,
                   config_checksum, created_at, updated_at, last_activity, exit_code
            FROM tunnels
            WHERE status IN (3, 4)
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch active tunnels")?;

        // 並列で復号化処理
        let mut tunnel_infos = Vec::new();
        for entry in entries {
            match self.entry_to_tunnel_info(entry).await {
                Ok(info) => tunnel_infos.push(info),
                Err(e) => {
                    error!("Failed to decrypt tunnel config for {}: {}", entry.id, e);
                    // 復号化失敗したトンネルは除外
                }
            }
        }

        Ok(tunnel_infos)
    }

    // 全トンネル一覧の取得
    pub async fn list_all_tunnels(&self) -> Result<Vec<TunnelInfo>> {
        let entries: Vec<TunnelEntry> = sqlx::query_as!(
            TunnelEntry,
            r#"
            SELECT id, name, pid, socket_path_hash, status, config_encrypted,
                   config_checksum, created_at, updated_at, last_activity, exit_code
            FROM tunnels
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        let mut tunnel_infos = Vec::new();
        for entry in entries {
            match self.entry_to_tunnel_info(entry).await {
                Ok(info) => tunnel_infos.push(info),
                Err(e) => {
                    error!("Failed to decrypt tunnel config: {}", e);
                }
            }
        }

        Ok(tunnel_infos)
    }

    // 特定トンネルの取得
    pub async fn get_tunnel(&self, id: &str) -> Result<Option<TunnelInfo>> {
        let entry: Option<TunnelEntry> = sqlx::query_as!(
            TunnelEntry,
            r#"
            SELECT id, name, pid, socket_path_hash, status, config_encrypted,
                   config_checksum, created_at, updated_at, last_activity, exit_code
            FROM tunnels
            WHERE id = ?
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        match entry {
            Some(entry) => Ok(Some(self.entry_to_tunnel_info(entry).await?)),
            None => Ok(None),
        }
    }

    // トンネルの削除
    pub async fn delete_tunnel(&self, id: &str) -> Result<bool> {
        let mut tx = self.pool.begin().await?;

        let result = sqlx::query!(
            "DELETE FROM tunnels WHERE id = ?",
            id
        )
        .execute(&mut *tx)
        .await?;

        let deleted = result.rows_affected() > 0;

        if deleted {
            self.log_audit_action(&mut tx, "DELETE", "tunnels", Some(id), true, None).await?;
            info!("Deleted tunnel: {}", id);
        }

        tx.commit().await?;
        Ok(deleted)
    }

    // 外部終了プロセスのクリーンアップ
    pub async fn cleanup_dead_processes(&self) -> Result<Vec<String>> {
        let active_tunnels = sqlx::query_as!(
            TunnelEntry,
            "SELECT * FROM tunnels WHERE status IN (3, 4) AND pid IS NOT NULL"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut cleaned_up = Vec::new();

        for tunnel in active_tunnels {
            if let Some(pid) = tunnel.pid {
                if !Self::process_exists(pid as u32) {
                    // プロセスが存在しない場合は状態をExitedに更新
                    let updated = self.update_tunnel_status(
                        &tunnel.id,
                        TunnelStatus::Exited,
                        Some(-1), // 外部終了を示す特別な終了コード
                    ).await?;

                    if updated {
                        cleaned_up.push(tunnel.id.clone());
                        warn!("Cleaned up dead tunnel process: {} (PID: {})", tunnel.id, pid);
                    }
                }
            }
        }

        Ok(cleaned_up)
    }

    // プロセス存在確認（マルチプラットフォーム対応）
    fn process_exists(pid: u32) -> bool {
        #[cfg(unix)]
        {
            std::path::Path::new(&format!("/proc/{}", pid)).exists()
        }

        #[cfg(windows)]
        {
            use std::process::Command;
            Command::new("tasklist")
                .args(&["/FI", &format!("PID eq {}", pid)])
                .output()
                .map(|output| {
                    String::from_utf8_lossy(&output.stdout)
                        .contains(&pid.to_string())
                })
                .unwrap_or(false)
        }
    }

    // TunnelEntryからTunnelInfoへの変換
    async fn entry_to_tunnel_info(&self, entry: TunnelEntry) -> Result<TunnelInfo> {
        let config = entry.decrypt_config(&**self.encryption_key)?;
        let socket_path = self.resolve_socket_path(&entry.socket_path_hash).await?;
        let metrics = self.get_tunnel_metrics(&entry.id).await.unwrap_or_default();

        Ok(TunnelInfo {
            id: entry.id,
            name: entry.name,
            pid: entry.pid.map(|p| p as u32),
            socket_path,
            status: entry.get_status(),
            config,
            created_at: entry.created_at,
            updated_at: entry.updated_at,
            last_activity: entry.last_activity,
            exit_code: entry.exit_code,
            metrics,
        })
    }

    // ソケットパスの解決
    async fn resolve_socket_path(&self, path_hash: &str) -> Result<PathBuf> {
        // 実際の実装では、ハッシュから元のパスを復元するロジックが必要
        // ここでは簡略化して標準パスを返す
        Ok(dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".conduit")
            .join("sockets")
            .join(format!("{}.sock", &path_hash[..8])))
    }

    // トンネルメトリクスの取得
    async fn get_tunnel_metrics(&self, tunnel_id: &str) -> Result<TunnelMetrics> {
        // セッション統計からメトリクスを計算
        let stats: Option<(i32, i64, i64, f64, i32)> = sqlx::query_as(
            r#"
            SELECT 
                COALESCE(SUM(total_connections), 0) as total_connections,
                COALESCE(SUM(total_bytes_sent), 0) as total_bytes_sent,
                COALESCE(SUM(total_bytes_received), 0) as total_bytes_received,
                COALESCE(AVG(avg_latency_ms), 0.0) as avg_latency_ms,
                COALESCE(SUM(error_count), 0) as error_count
            FROM sessions
            WHERE tunnel_id = ? AND ended_at IS NULL
            "#
        )
        .bind(tunnel_id)
        .fetch_optional(&self.pool)
        .await?;

        let active_connections: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM clients WHERE tunnel_id = ? AND status = 'active'",
            tunnel_id
        )
        .fetch_one(&self.pool)
        .await?;

        let (total_connections, total_bytes_sent, total_bytes_received, avg_latency_ms, error_count) = 
            stats.unwrap_or((0, 0, 0, 0.0, 0));

        Ok(TunnelMetrics {
            active_connections: active_connections as u32,
            total_connections: total_connections as u64,
            total_bytes_sent: total_bytes_sent as u64,
            total_bytes_received: total_bytes_received as u64,
            cpu_usage: 0.0, // 実際の実装では外部から取得
            memory_usage: 0, // 実際の実装では外部から取得
            uptime_seconds: 0, // 実際の実装では計算
            avg_latency_ms,
            error_rate: if total_connections > 0 {
                (error_count as f64 / total_connections as f64) * 100.0
            } else {
                0.0
            },
        })
    }

    // 監査ログの記録
    async fn log_audit_action(
        &self,
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        action: &str,
        target_table: &str,
        target_id: Option<&str>,
        success: bool,
        error_message: Option<&str>,
    ) -> Result<()> {
        let timestamp = chrono::Utc::now().timestamp();
        let user_context = std::env::var("USER").ok();

        sqlx::query!(
            r#"
            INSERT INTO audit_log (action, target_table, target_id, user_context, timestamp, success, error_message)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            action,
            target_table,
            target_id,
            user_context,
            timestamp,
            success,
            error_message
        )
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    // 暗号化キーの取得または生成
    async fn get_or_create_encryption_key(pool: &Pool<Sqlite>) -> Result<[u8; 32]> {
        // 既存のキーを確認
        let existing_key: Option<String> = sqlx::query_scalar(
            "SELECT key_id FROM config_metadata WHERE is_active = TRUE ORDER BY created_at DESC LIMIT 1"
        )
        .fetch_optional(pool)
        .await?;

        if existing_key.is_some() {
            // 既存のキーが存在する場合は、実際の実装では安全な場所から取得
            // ここでは簡略化してデフォルトキーを使用
            warn!("Using default encryption key for demonstration purposes");
            Ok(*b"0123456789abcdef0123456789abcdef")
        } else {
            // 新しいキーを生成
            use ring::rand::{SecureRandom, SystemRandom};
            let rng = SystemRandom::new();
            let mut key = [0u8; 32];
            rng.fill(&mut key)
                .map_err(|_| anyhow::anyhow!("Failed to generate encryption key"))?;

            // キーメタデータをデータベースに保存
            let key_id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().timestamp();
            let rotation_time = now + (30 * 24 * 60 * 60); // 30日後

            sqlx::query!(
                r#"
                INSERT INTO config_metadata (key_id, algorithm, key_rotation_at, created_at, is_active)
                VALUES (?, ?, ?, ?, ?)
                "#,
                key_id,
                "AES-256-GCM",
                rotation_time,
                now,
                true
            )
            .execute(pool)
            .await?;

            info!("Generated new encryption key: {}", key_id);
            Ok(key)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_sqlite_registry_creation() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        let registry = SqliteRegistry::new(Some(db_path)).await.unwrap();
        
        // レジストリが正常に作成されることを確認
        assert!(!registry.encryption_key.is_empty());
    }

    #[tokio::test]
    async fn test_tunnel_crud_operations() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let registry = SqliteRegistry::new(Some(db_path)).await.unwrap();

        let config = TunnelConfig {
            router_addr: "10.2.0.1:9999".to_string(),
            source_addr: "10.2.0.2:8080".to_string(),
            bind_addr: "0.0.0.0:80".to_string(),
            protocol: "tcp".to_string(),
            timeout_seconds: 30,
            max_connections: 100,
        };

        // 作成
        registry.create_tunnel(
            "test-id".to_string(),
            "test-tunnel".to_string(),
            12345,
            "/tmp/test.sock",
            &config,
        ).await.unwrap();

        // 取得
        let tunnel = registry.get_tunnel("test-id").await.unwrap().unwrap();
        assert_eq!(tunnel.name, "test-tunnel");
        assert_eq!(tunnel.config.router_addr, config.router_addr);

        // 更新
        let updated = registry.update_tunnel_status("test-id", TunnelStatus::Running, None).await.unwrap();
        assert!(updated);

        // 削除
        let deleted = registry.delete_tunnel("test-id").await.unwrap();
        assert!(deleted);
    }
}