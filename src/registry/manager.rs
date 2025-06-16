// プロセス管理・監視機能
// 軽量Tunnel Processの起動・監視・クリーンアップ

use crate::registry::{models::*, sqlite::SqliteRegistry};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Mutex};
use tokio::time::{interval, sleep};
use tracing::{debug, error, info, warn};

// プロセス管理構造体
pub struct ProcessManager {
    registry: Arc<SqliteRegistry>,
    running_processes: Arc<RwLock<HashMap<String, ProcessInfo>>>,
    cleanup_interval: Duration,
    health_check_interval: Duration,
}

// 実行中プロセス情報
#[derive(Debug, Clone)]
struct ProcessInfo {
    tunnel_id: String,
    process_handle: Arc<Mutex<Option<Arc<Mutex<Child>>>>>,
    socket_path: PathBuf,
    started_at: Instant,
    last_health_check: Instant,
    restart_count: u32,
}

impl ProcessManager {
    // 新しいプロセス管理インスタンスの作成
    pub fn new(registry: Arc<SqliteRegistry>) -> Self {
        Self {
            registry,
            running_processes: Arc::new(RwLock::new(HashMap::new())),
            cleanup_interval: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(10),
        }
    }

    // 軽量Tunnel Processの起動
    pub async fn start_tunnel_process(
        &self,
        tunnel_id: String,
        name: String,
        config: &TunnelConfig,
    ) -> Result<u32> {
        info!("Starting tunnel process: {} ({})", name, tunnel_id);

        // ソケットパスの準備
        let socket_path = self.prepare_socket_path(&tunnel_id).await?;
        
        // コマンドライン引数の構築
        let mut cmd = Command::new(std::env::current_exe()?);
        cmd.args(&[
            "internal-tunnel-process",
            "--id", &tunnel_id,
            "--name", &name,
            "--router", &config.router_addr,
            "--source", &config.source_addr,
            "--bind", &config.bind_addr,
            "--socket", &socket_path.to_string_lossy(),
            "--protocol", &config.protocol,
            "--timeout", &config.timeout_seconds.to_string(),
            "--max-connections", &config.max_connections.to_string(),
        ]);

        // プロセス起動設定（conmonパターン）
        cmd.stdin(Stdio::null())
           .stdout(Stdio::piped())
           .stderr(Stdio::piped());

        // 環境変数設定
        cmd.env("CONDUIT_TUNNEL_ID", &tunnel_id);
        cmd.env("CONDUIT_SOCKET_PATH", &socket_path);

        // プロセス起動
        let mut child = cmd.spawn()
            .context("Failed to spawn tunnel process")?;

        let pid = child.id();
        debug!("Spawned tunnel process with PID: {}", pid);

        // レジストリにトンネル情報を登録
        self.registry.create_tunnel(
            tunnel_id.clone(),
            name,
            pid as i32,
            &socket_path.to_string_lossy(),
            config,
        ).await?;

        // プロセス情報を管理対象に追加
        let process_info = ProcessInfo {
            tunnel_id: tunnel_id.clone(),
            process_handle: Arc::new(Mutex::new(Some(Arc::new(Mutex::new(child))))),
            socket_path,
            started_at: Instant::now(),
            last_health_check: Instant::now(),
            restart_count: 0,
        };

        self.running_processes.write().await.insert(tunnel_id, process_info);
        
        Ok(pid)
    }

    // プロセス停止
    pub async fn stop_tunnel_process(&self, tunnel_id: &str, force: bool) -> Result<bool> {
        info!("Stopping tunnel process: {} (force: {})", tunnel_id, force);

        let process_info = {
            let processes = self.running_processes.read().await;
            processes.get(tunnel_id).cloned()
        };

        if let Some(info) = process_info {
            // レジストリ状態を停止中に更新
            self.registry.update_tunnel_status(
                tunnel_id,
                TunnelStatus::Stopping,
                None,
            ).await?;

            let mut child_guard = info.process_handle.lock().await;
            if let Some(ref mut child) = child_guard.as_mut() {
                if force {
                    // 強制終了
                    child.kill().context("Failed to kill tunnel process")?;
                } else {
                    // グレースフル停止の試行
                    #[cfg(unix)]
                    {
                        use nix::sys::signal::{kill, Signal};
                        use nix::unistd::Pid;
                        
                        if let Some(pid) = child.id() {
                            kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
                                .context("Failed to send SIGTERM")?;
                            
                            // グレースフル停止の待機（最大10秒）
                            let mut attempts = 0;
                            while attempts < 10 {
                                match child.try_wait()? {
                                    Some(_) => break,
                                    None => {
                                        sleep(Duration::from_millis(1000)).await;
                                        attempts += 1;
                                    }
                                }
                            }
                            
                            // まだ生きている場合は強制終了
                            if child.try_wait()?.is_none() {
                                warn!("Process {} did not respond to SIGTERM, killing", tunnel_id);
                                child.kill()?;
                            }
                        }
                    }
                    
                    #[cfg(windows)]
                    {
                        // Windows版では直接killを使用
                        child.kill().context("Failed to kill tunnel process")?;
                    }
                }

                // プロセス終了の待機
                let exit_status = child.wait().context("Failed to wait for process exit")?;
                let exit_code = exit_status.code().unwrap_or(-1);

                // レジストリ状態を終了に更新
                self.registry.update_tunnel_status(
                    tunnel_id,
                    if exit_code == 0 { TunnelStatus::Exited } else { TunnelStatus::Error },
                    Some(exit_code),
                ).await?;

                info!("Tunnel process {} stopped with exit code: {}", tunnel_id, exit_code);
            }

            // プロセス情報を削除
            self.running_processes.write().await.remove(tunnel_id);
            
            // ソケットファイルのクリーンアップ
            let _ = tokio::fs::remove_file(&info.socket_path).await;

            Ok(true)
        } else {
            warn!("Tunnel process {} not found in running processes", tunnel_id);
            Ok(false)
        }
    }

    // 全プロセス停止
    pub async fn stop_all_processes(&self, force: bool) -> Result<Vec<String>> {
        let tunnel_ids: Vec<String> = {
            let processes = self.running_processes.read().await;
            processes.keys().cloned().collect()
        };

        let mut stopped = Vec::new();
        for tunnel_id in tunnel_ids {
            match self.stop_tunnel_process(&tunnel_id, force).await {
                Ok(true) => stopped.push(tunnel_id),
                Ok(false) => {
                    warn!("Failed to stop process: {}", tunnel_id);
                }
                Err(e) => {
                    error!("Error stopping process {}: {}", tunnel_id, e);
                }
            }
        }

        Ok(stopped)
    }

    // プロセス監視とクリーンアップのバックグラウンドタスク開始
    pub async fn start_monitoring(&self) -> Result<()> {
        let registry = Arc::clone(&self.registry);
        let processes = Arc::clone(&self.running_processes);
        let cleanup_interval = self.cleanup_interval;
        let health_check_interval = self.health_check_interval;

        // クリーンアップタスク
        tokio::spawn(async move {
            let mut cleanup_timer = interval(cleanup_interval);
            loop {
                cleanup_timer.tick().await;
                
                if let Err(e) = Self::cleanup_dead_processes(&registry, &processes).await {
                    error!("Error during process cleanup: {}", e);
                }
            }
        });

        // ヘルスチェックタスク
        let registry_health = Arc::clone(&self.registry);
        let processes_health = Arc::clone(&self.running_processes);
        tokio::spawn(async move {
            let mut health_timer = interval(health_check_interval);
            loop {
                health_timer.tick().await;
                
                if let Err(e) = Self::health_check_processes(&registry_health, &processes_health).await {
                    error!("Error during health check: {}", e);
                }
            }
        });

        info!("Process monitoring started");
        Ok(())
    }

    // デッドプロセスのクリーンアップ
    async fn cleanup_dead_processes(
        registry: &SqliteRegistry,
        processes: &RwLock<HashMap<String, ProcessInfo>>,
    ) -> Result<()> {
        // データベースから外部終了したプロセスをクリーンアップ
        let cleaned_ids = registry.cleanup_dead_processes().await?;
        
        // メモリ内のプロセス情報もクリーンアップ
        if !cleaned_ids.is_empty() {
            let mut processes_guard = processes.write().await;
            for id in &cleaned_ids {
                if let Some(info) = processes_guard.remove(id) {
                    // ソケットファイルのクリーンアップ
                    let _ = tokio::fs::remove_file(&info.socket_path).await;
                    debug!("Cleaned up process info for: {}", id);
                }
            }
        }

        // 実行中プロセスの生存確認
        let mut dead_processes = Vec::new();
        {
            let mut processes_guard = processes.write().await;
            let mut to_remove = Vec::new();
            
            for (tunnel_id, info) in processes_guard.iter_mut() {
                let mut child_guard = info.process_handle.lock().await;
                if let Some(ref mut child) = child_guard.as_mut() {
                    match child.try_wait() {
                        Ok(Some(exit_status)) => {
                            // プロセスが終了している
                            let exit_code = exit_status.code().unwrap_or(-1);
                            dead_processes.push((tunnel_id.clone(), exit_code));
                            to_remove.push(tunnel_id.clone());
                        }
                        Ok(None) => {
                            // プロセスはまだ実行中
                        }
                        Err(_) => {
                            // プロセス状態取得エラー（おそらく終了済み）
                            dead_processes.push((tunnel_id.clone(), -1));
                            to_remove.push(tunnel_id.clone());
                        }
                    }
                }
            }
            
            for id in to_remove {
                processes_guard.remove(&id);
            }
        }

        // デッドプロセスのレジストリ状態更新
        for (tunnel_id, exit_code) in dead_processes {
            let status = if exit_code == 0 { TunnelStatus::Exited } else { TunnelStatus::Error };
            if let Err(e) = registry.update_tunnel_status(&tunnel_id, status, Some(exit_code)).await {
                error!("Failed to update status for dead process {}: {}", tunnel_id, e);
            } else {
                info!("Cleaned up dead process: {} (exit code: {})", tunnel_id, exit_code);
            }
        }

        Ok(())
    }

    // プロセスのヘルスチェック
    async fn health_check_processes(
        registry: &SqliteRegistry,
        processes: &RwLock<HashMap<String, ProcessInfo>>,
    ) -> Result<()> {
        let process_ids: Vec<String> = {
            let processes_guard = processes.read().await;
            processes_guard.keys().cloned().collect()
        };

        for tunnel_id in process_ids {
            // UDS接続の確認を通じたヘルスチェック
            // 実際の実装では、各プロセスのgRPCエンドポイントに接続してpingを送信
            if let Err(e) = Self::check_process_health(&tunnel_id).await {
                warn!("Health check failed for process {}: {}", tunnel_id, e);
                
                // ヘルスチェック失敗時の処理（必要に応じてプロセス再起動）
                // ここでは警告ログのみ
            }
        }

        Ok(())
    }

    // 個別プロセスのヘルスチェック
    async fn check_process_health(tunnel_id: &str) -> Result<()> {
        // TODO: UDS gRPCクライアントを使用したヘルスチェック実装
        // 現在は簡略化
        debug!("Health check for process: {}", tunnel_id);
        Ok(())
    }

    // ソケットパスの準備
    async fn prepare_socket_path(&self, tunnel_id: &str) -> Result<PathBuf> {
        let socket_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".conduit")
            .join("sockets");

        // ディレクトリ作成
        tokio::fs::create_dir_all(&socket_dir).await
            .context("Failed to create socket directory")?;

        // Unix系でのディレクトリ権限設定
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o700);
            std::fs::set_permissions(&socket_dir, perms)
                .context("Failed to set socket directory permissions")?;
        }

        let socket_path = socket_dir.join(format!("{}.sock", tunnel_id));
        
        // 既存のソケットファイルを削除
        if socket_path.exists() {
            tokio::fs::remove_file(&socket_path).await
                .context("Failed to remove existing socket file")?;
        }

        Ok(socket_path)
    }

    // 実行中プロセス一覧の取得
    pub async fn list_running_processes(&self) -> Vec<String> {
        let processes = self.running_processes.read().await;
        processes.keys().cloned().collect()
    }

    // プロセス統計情報の取得
    pub async fn get_process_stats(&self) -> HashMap<String, ProcessStats> {
        let processes = self.running_processes.read().await;
        let mut stats = HashMap::new();
        
        for (tunnel_id, info) in processes.iter() {
            let uptime = info.started_at.elapsed();
            let process_stats = ProcessStats {
                tunnel_id: tunnel_id.clone(),
                pid: {
                    let child_guard = info.process_handle.lock().await;
                    child_guard.as_ref().and_then(|c| c.id()).map(|id| id as u32)
                },
                uptime_seconds: uptime.as_secs(),
                restart_count: info.restart_count,
                last_health_check: info.last_health_check.elapsed().as_secs(),
                socket_path: info.socket_path.clone(),
            };
            stats.insert(tunnel_id.clone(), process_stats);
        }
        
        stats
    }
}

// プロセス統計情報
#[derive(Debug, Clone)]
pub struct ProcessStats {
    pub tunnel_id: String,
    pub pid: Option<u32>,
    pub uptime_seconds: u64,
    pub restart_count: u32,
    pub last_health_check: u64,
    pub socket_path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_process_manager_creation() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let registry = Arc::new(SqliteRegistry::new(Some(db_path)).await.unwrap());
        
        let manager = ProcessManager::new(registry);
        assert_eq!(manager.cleanup_interval, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_socket_path_preparation() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let registry = Arc::new(SqliteRegistry::new(Some(db_path)).await.unwrap());
        let manager = ProcessManager::new(registry);

        let socket_path = manager.prepare_socket_path("test-tunnel").await.unwrap();
        assert!(socket_path.to_string_lossy().contains("test-tunnel.sock"));
    }
}