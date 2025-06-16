// Process Registry統合管理
// SQLite Registry + プロセス管理の統合インターフェース

pub mod models;
pub mod sqlite;
pub mod manager;

use crate::registry::{
    models::*,
    sqlite::SqliteRegistry,
    manager::{ProcessManager, ProcessStats},
};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info};

// Process Registry統合管理構造体
pub struct ProcessRegistry {
    sqlite_registry: Arc<SqliteRegistry>,
    process_manager: ProcessManager,
}

impl ProcessRegistry {
    // 新しいProcess Registryインスタンスの作成
    pub async fn new(db_path: Option<PathBuf>) -> Result<Self> {
        info!("Initializing Process Registry system");
        
        let sqlite_registry = Arc::new(SqliteRegistry::new(db_path).await?);
        let process_manager = ProcessManager::new(Arc::clone(&sqlite_registry));

        // プロセス監視の開始
        process_manager.start_monitoring().await?;

        Ok(Self {
            sqlite_registry,
            process_manager,
        })
    }

    // トンネルの作成と起動
    pub async fn create_and_start_tunnel(
        &self,
        tunnel_id: String,
        name: String,
        config: TunnelConfig,
    ) -> Result<u32> {
        debug!("Creating and starting tunnel: {} ({})", name, tunnel_id);
        
        // プロセス起動とレジストリ登録を同時実行
        let pid = self.process_manager.start_tunnel_process(
            tunnel_id.clone(),
            name,
            &config,
        ).await?;

        info!("Successfully started tunnel: {} with PID: {}", tunnel_id, pid);
        Ok(pid)
    }

    // トンネルの停止
    pub async fn stop_tunnel(&self, tunnel_id: &str, force: bool) -> Result<bool> {
        debug!("Stopping tunnel: {} (force: {})", tunnel_id, force);
        
        let stopped = self.process_manager.stop_tunnel_process(tunnel_id, force).await?;
        
        if stopped {
            info!("Successfully stopped tunnel: {}", tunnel_id);
        }
        
        Ok(stopped)
    }

    // 全トンネル停止
    pub async fn stop_all_tunnels(&self, force: bool) -> Result<Vec<String>> {
        info!("Stopping all tunnels (force: {})", force);
        
        let stopped = self.process_manager.stop_all_processes(force).await?;
        
        info!("Stopped {} tunnels", stopped.len());
        Ok(stopped)
    }

    // アクティブトンネル一覧の取得
    pub async fn list_active_tunnels(&self) -> Result<Vec<TunnelInfo>> {
        debug!("Retrieving active tunnel list");
        self.sqlite_registry.list_active_tunnels().await
    }

    // 全トンネル一覧の取得
    pub async fn list_all_tunnels(&self) -> Result<Vec<TunnelInfo>> {
        debug!("Retrieving all tunnel list");
        self.sqlite_registry.list_all_tunnels().await
    }

    // 特定トンネルの取得
    pub async fn get_tunnel(&self, tunnel_id: &str) -> Result<Option<TunnelInfo>> {
        debug!("Retrieving tunnel info: {}", tunnel_id);
        self.sqlite_registry.get_tunnel(tunnel_id).await
    }

    // トンネル状態の更新
    pub async fn update_tunnel_status(
        &self,
        tunnel_id: &str,
        status: TunnelStatus,
        exit_code: Option<i32>,
    ) -> Result<bool> {
        debug!("Updating tunnel status: {} -> {}", tunnel_id, status.as_str());
        self.sqlite_registry.update_tunnel_status(tunnel_id, status, exit_code).await
    }

    // トンネルの削除
    pub async fn delete_tunnel(&self, tunnel_id: &str) -> Result<bool> {
        debug!("Deleting tunnel: {}", tunnel_id);
        
        // プロセス停止とレジストリ削除
        let _ = self.stop_tunnel(tunnel_id, true).await;
        let deleted = self.sqlite_registry.delete_tunnel(tunnel_id).await?;
        
        if deleted {
            info!("Successfully deleted tunnel: {}", tunnel_id);
        }
        
        Ok(deleted)
    }

    // デッドプロセスのクリーンアップ
    pub async fn cleanup_dead_processes(&self) -> Result<Vec<String>> {
        debug!("Running dead process cleanup");
        self.sqlite_registry.cleanup_dead_processes().await
    }

    // プロセス統計情報の取得
    pub async fn get_process_stats(&self) -> HashMap<String, ProcessStats> {
        debug!("Retrieving process statistics");
        self.process_manager.get_process_stats().await
    }

    // 実行中プロセス一覧の取得
    pub async fn list_running_processes(&self) -> Vec<String> {
        self.process_manager.list_running_processes().await
    }

    // システム統計情報の取得
    pub async fn get_system_stats(&self) -> Result<SystemStats> {
        let all_tunnels = self.list_all_tunnels().await?;
        let running_processes = self.list_running_processes().await;
        let process_stats = self.get_process_stats().await;

        let active_count = all_tunnels.iter()
            .filter(|t| t.status.is_active())
            .count() as u32;

        let total_connections: u64 = all_tunnels.iter()
            .map(|t| t.metrics.total_connections)
            .sum();

        let total_bytes_transferred: u64 = all_tunnels.iter()
            .map(|t| t.metrics.total_bytes_sent + t.metrics.total_bytes_received)
            .sum();

        let avg_uptime: f64 = if !process_stats.is_empty() {
            process_stats.values()
                .map(|s| s.uptime_seconds as f64)
                .sum::<f64>() / process_stats.len() as f64
        } else {
            0.0
        };

        Ok(SystemStats {
            total_tunnels: all_tunnels.len() as u32,
            active_tunnels: active_count,
            running_processes: running_processes.len() as u32,
            total_connections,
            total_bytes_transferred,
            avg_uptime_seconds: avg_uptime as u64,
            registry_version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    // ヘルスチェック
    pub async fn health_check(&self) -> Result<HealthStatus> {
        let system_stats = self.get_system_stats().await?;
        let process_stats = self.get_process_stats().await;
        
        // 基本的なヘルスチェック条件
        let is_healthy = system_stats.active_tunnels == system_stats.running_processes;
        
        let status = if is_healthy {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        };

        Ok(HealthStatus {
            status,
            timestamp: chrono::Utc::now().timestamp(),
            system_stats,
            process_count: process_stats.len() as u32,
            issues: if is_healthy { Vec::new() } else { 
                vec!["Process count mismatch".to_string()] 
            },
        })
    }
}

// システム統計情報
#[derive(Debug, Clone)]
pub struct SystemStats {
    pub total_tunnels: u32,
    pub active_tunnels: u32,
    pub running_processes: u32,
    pub total_connections: u64,
    pub total_bytes_transferred: u64,
    pub avg_uptime_seconds: u64,
    pub registry_version: String,
}

// ヘルスステータス
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub status: String,
    pub timestamp: i64,
    pub system_stats: SystemStats,
    pub process_count: u32,
    pub issues: Vec<String>,
}

// Process Registry用のユーティリティ関数
impl ProcessRegistry {
    // 設定ファイルからの一括トンネル作成
    pub async fn create_tunnels_from_config(
        &self,
        tunnels_config: Vec<(String, String, TunnelConfig)>, // (id, name, config)
    ) -> Result<Vec<(String, Result<u32>)>> {
        let mut results = Vec::new();
        
        for (tunnel_id, name, config) in tunnels_config {
            let result = self.create_and_start_tunnel(tunnel_id.clone(), name, config).await;
            results.push((tunnel_id, result));
        }
        
        Ok(results)
    }

    // パターンマッチングによるトンネル検索
    pub async fn find_tunnels_by_pattern(&self, pattern: &str) -> Result<Vec<TunnelInfo>> {
        let all_tunnels = self.list_all_tunnels().await?;
        
        let matching_tunnels = all_tunnels.into_iter()
            .filter(|tunnel| {
                tunnel.name.contains(pattern) || 
                tunnel.id.contains(pattern) ||
                tunnel.config.bind_addr.contains(pattern) ||
                tunnel.config.source_addr.contains(pattern)
            })
            .collect();
        
        Ok(matching_tunnels)
    }

    // メトリクス集約
    pub async fn aggregate_metrics(&self) -> Result<AggregatedMetrics> {
        let tunnels = self.list_all_tunnels().await?;
        
        let mut total_active_connections = 0u32;
        let mut total_connections = 0u64;
        let mut total_bytes_sent = 0u64;
        let mut total_bytes_received = 0u64;
        let mut total_uptime = 0u64;
        let mut latency_sum = 0f64;
        let mut error_count_sum = 0f64;
        let mut tunnel_count = 0u32;
        
        for tunnel in &tunnels {
            total_active_connections += tunnel.metrics.active_connections;
            total_connections += tunnel.metrics.total_connections;
            total_bytes_sent += tunnel.metrics.total_bytes_sent;
            total_bytes_received += tunnel.metrics.total_bytes_received;
            total_uptime += tunnel.metrics.uptime_seconds;
            latency_sum += tunnel.metrics.avg_latency_ms;
            error_count_sum += tunnel.metrics.error_rate;
            tunnel_count += 1;
        }
        
        Ok(AggregatedMetrics {
            total_active_connections,
            total_connections,
            total_bytes_sent,
            total_bytes_received,
            avg_latency_ms: if tunnel_count > 0 { latency_sum / tunnel_count as f64 } else { 0.0 },
            avg_error_rate: if tunnel_count > 0 { error_count_sum / tunnel_count as f64 } else { 0.0 },
            avg_uptime_seconds: if tunnel_count > 0 { total_uptime / tunnel_count as u64 } else { 0 },
            tunnel_count,
        })
    }
}

// 集約メトリクス
#[derive(Debug, Clone)]
pub struct AggregatedMetrics {
    pub total_active_connections: u32,
    pub total_connections: u64,
    pub total_bytes_sent: u64,
    pub total_bytes_received: u64,
    pub avg_latency_ms: f64,
    pub avg_error_rate: f64,
    pub avg_uptime_seconds: u64,
    pub tunnel_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_process_registry_creation() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        let registry = ProcessRegistry::new(Some(db_path)).await.unwrap();
        
        // 初期状態の確認
        let stats = registry.get_system_stats().await.unwrap();
        assert_eq!(stats.total_tunnels, 0);
        assert_eq!(stats.active_tunnels, 0);
    }

    #[tokio::test]
    async fn test_tunnel_lifecycle() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let registry = ProcessRegistry::new(Some(db_path)).await.unwrap();

        let config = TunnelConfig {
            router_addr: "10.2.0.1:9999".to_string(),
            source_addr: "10.2.0.2:8080".to_string(),
            bind_addr: "0.0.0.0:80".to_string(),
            protocol: "tcp".to_string(),
            timeout_seconds: 30,
            max_connections: 100,
        };

        // NOTE: 実際のプロセス起動はテスト環境では困難なため、
        // データベース操作のみをテスト
        let tunnel_info = registry.get_tunnel("nonexistent").await.unwrap();
        assert!(tunnel_info.is_none());
    }
}