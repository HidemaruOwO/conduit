// UDS gRPCクライアント
// CLI CommandsからTunnel Processへの通信クライアント

use crate::ipc::protocol::{tunnel::*, create_uds_channel, TunnelControlClient};
use anyhow::{Context, Result};
use std::path::Path;
use std::time::Duration;
use tokio::time::timeout;
use tokio_stream::StreamExt;
use tonic::Request;
use tracing::{debug, error, warn};

// UDS gRPCクライアント
pub struct UdsGrpcClient {
    client: TunnelControlClient<tonic::transport::Channel>,
    socket_path: std::path::PathBuf,
}

impl UdsGrpcClient {
    // UDSへの接続
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        debug!("Connecting to UDS gRPC server: {}", socket_path.display());

        // ソケットファイルの存在確認
        if !socket_path.exists() {
            return Err(anyhow::anyhow!(
                "Socket file not found: {}",
                socket_path.display()
            ));
        }

        // UDS接続の確立
        let channel = create_uds_channel(socket_path)
            .await
            .context("Failed to create UDS channel")?;

        let client = TunnelControlClient::new(channel);

        Ok(Self {
            client,
            socket_path: socket_path.to_path_buf(),
        })
    }

    // タイムアウト付きでの接続
    pub async fn connect_with_timeout(
        socket_path: &Path,
        timeout_duration: Duration,
    ) -> Result<Self> {
        timeout(timeout_duration, Self::connect(socket_path))
            .await
            .context("Connection timeout")?
    }

    // トンネル状態の取得
    pub async fn get_status(&mut self) -> Result<StatusResponse> {
        debug!("Requesting tunnel status from: {}", self.socket_path.display());

        let request = Request::new(StatusRequest {});
        
        let response = self
            .client
            .get_status(request)
            .await
            .map_err(|e| {
                error!("Failed to get status: {}", e);
                anyhow::anyhow!("gRPC error: {}", e)
            })?;

        Ok(response.into_inner())
    }

    // タイムアウト付きでの状態取得
    pub async fn get_status_with_timeout(
        &mut self,
        timeout_duration: Duration,
    ) -> Result<StatusResponse> {
        timeout(timeout_duration, self.get_status())
            .await
            .context("Status request timeout")?
    }

    // アクティブ接続一覧の取得
    pub async fn list_connections(&mut self) -> Result<ListResponse> {
        debug!("Requesting connection list from: {}", self.socket_path.display());

        let request = Request::new(ListRequest {});
        
        let response = self
            .client
            .list_connections(request)
            .await
            .map_err(|e| {
                error!("Failed to list connections: {}", e);
                anyhow::anyhow!("gRPC error: {}", e)
            })?;

        Ok(response.into_inner())
    }

    // トンネルの停止
    pub async fn shutdown(&mut self, force: bool, timeout_seconds: i32) -> Result<ShutdownResponse> {
        debug!(
            "Requesting tunnel shutdown: force={}, timeout={}s",
            force, timeout_seconds
        );

        let request = Request::new(ShutdownRequest {
            force,
            timeout_seconds,
        });

        let response = self
            .client
            .shutdown(request)
            .await
            .map_err(|e| {
                error!("Failed to shutdown tunnel: {}", e);
                anyhow::anyhow!("gRPC error: {}", e)
            })?;

        let shutdown_response = response.into_inner();
        
        if shutdown_response.success {
            debug!("Tunnel shutdown successful: {}", shutdown_response.message);
        } else {
            warn!("Tunnel shutdown failed: {}", shutdown_response.message);
        }

        Ok(shutdown_response)
    }

    // メトリクスストリームの取得
    pub async fn get_metrics_stream(&mut self) -> Result<impl StreamExt<Item = Result<MetricsResponse, tonic::Status>>> {
        debug!("Starting metrics stream from: {}", self.socket_path.display());

        let request = Request::new(MetricsRequest {});
        
        let response = self
            .client
            .get_metrics_stream(request)
            .await
            .map_err(|e| {
                error!("Failed to start metrics stream: {}", e);
                anyhow::anyhow!("gRPC error: {}", e)
            })?;

        Ok(response.into_inner())
    }

    // 限定されたメトリクス取得（指定回数分）
    pub async fn get_metrics_limited(&mut self, count: usize) -> Result<Vec<MetricsResponse>> {
        let mut stream = self.get_metrics_stream().await?;
        let mut metrics = Vec::with_capacity(count);

        for _ in 0..count {
            if let Some(result) = stream.next().await {
                match result {
                    Ok(metric) => metrics.push(metric),
                    Err(e) => {
                        error!("Error receiving metrics: {}", e);
                        break;
                    }
                }
            } else {
                break;
            }
        }

        Ok(metrics)
    }

    // ping機能（ヘルスチェック用）
    pub async fn ping(&mut self) -> Result<()> {
        debug!("Pinging tunnel server: {}", self.socket_path.display());

        // StatusRequestを使用してpingを実装
        match self.get_status_with_timeout(Duration::from_millis(1000)).await {
            Ok(_) => Ok(()),
            Err(e) => {
                warn!("Ping failed: {}", e);
                Err(e)
            }
        }
    }

    // 接続の再確立
    pub async fn reconnect(&mut self) -> Result<()> {
        debug!("Reconnecting to: {}", self.socket_path.display());

        let channel = create_uds_channel(&self.socket_path)
            .await
            .context("Failed to reconnect UDS channel")?;

        self.client = TunnelControlClient::new(channel);
        Ok(())
    }

    // 接続状態の確認
    pub async fn is_connected(&mut self) -> bool {
        self.ping().await.is_ok()
    }
}

// 並列クライアント管理
pub struct ParallelUdsClient;

impl ParallelUdsClient {
    // 複数のトンネルから並列で状態を取得（100並列対応）
    pub async fn get_multiple_status(
        socket_paths: Vec<std::path::PathBuf>,
        timeout_ms: u64,
    ) -> Vec<(std::path::PathBuf, Result<StatusResponse>)> {
        let timeout_duration = Duration::from_millis(timeout_ms);
        
        // 100並列での非同期処理
        let tasks: Vec<_> = socket_paths
            .into_iter()
            .map(|socket_path| {
                let timeout_duration = timeout_duration;
                tokio::spawn(async move {
                    let result = Self::get_single_status(&socket_path, timeout_duration).await;
                    (socket_path, result)
                })
            })
            .collect();

        // 全タスクの完了を待機
        let mut results = Vec::new();
        for task in tasks {
            match task.await {
                Ok(result) => results.push(result),
                Err(e) => {
                    error!("Task join error: {}", e);
                }
            }
        }

        results
    }

    // 単一トンネルの状態取得
    async fn get_single_status(
        socket_path: &Path,
        timeout_duration: Duration,
    ) -> Result<StatusResponse> {
        match UdsGrpcClient::connect_with_timeout(socket_path, timeout_duration).await {
            Ok(mut client) => {
                client.get_status_with_timeout(timeout_duration).await
            }
            Err(e) => {
                debug!("Failed to connect to {}: {}", socket_path.display(), e);
                Err(e)
            }
        }
    }

    // ヘルスチェック（複数トンネル並列）
    pub async fn health_check_multiple(
        socket_paths: Vec<std::path::PathBuf>,
        timeout_ms: u64,
    ) -> Vec<(std::path::PathBuf, bool)> {
        let timeout_duration = Duration::from_millis(timeout_ms);
        
        let tasks: Vec<_> = socket_paths
            .into_iter()
            .map(|socket_path| {
                tokio::spawn(async move {
                    let is_healthy = Self::health_check_single(&socket_path, timeout_duration).await;
                    (socket_path, is_healthy)
                })
            })
            .collect();

        let mut results = Vec::new();
        for task in tasks {
            if let Ok(result) = task.await {
                results.push(result);
            }
        }

        results
    }

    // 単一トンネルのヘルスチェック
    async fn health_check_single(socket_path: &Path, timeout_duration: Duration) -> bool {
        match UdsGrpcClient::connect_with_timeout(socket_path, timeout_duration).await {
            Ok(mut client) => client.ping().await.is_ok(),
            Err(_) => false,
        }
    }

    // 一括停止
    pub async fn shutdown_multiple(
        socket_paths: Vec<std::path::PathBuf>,
        force: bool,
        timeout_seconds: i32,
        operation_timeout_ms: u64,
    ) -> Vec<(std::path::PathBuf, Result<ShutdownResponse>)> {
        let timeout_duration = Duration::from_millis(operation_timeout_ms);
        
        let tasks: Vec<_> = socket_paths
            .into_iter()
            .map(|socket_path| {
                tokio::spawn(async move {
                    let result = Self::shutdown_single(&socket_path, force, timeout_seconds, timeout_duration).await;
                    (socket_path, result)
                })
            })
            .collect();

        let mut results = Vec::new();
        for task in tasks {
            if let Ok(result) = task.await {
                results.push(result);
            }
        }

        results
    }

    // 単一トンネルの停止
    async fn shutdown_single(
        socket_path: &Path,
        force: bool,
        timeout_seconds: i32,
        timeout_duration: Duration,
    ) -> Result<ShutdownResponse> {
        match UdsGrpcClient::connect_with_timeout(socket_path, timeout_duration).await {
            Ok(mut client) => {
                client.shutdown(force, timeout_seconds).await
            }
            Err(e) => {
                debug!("Failed to connect to {}: {}", socket_path.display(), e);
                Err(e)
            }
        }
    }
}

// Client Pool管理（接続再利用）
pub struct UdsClientPool {
    clients: std::collections::HashMap<std::path::PathBuf, UdsGrpcClient>,
}

impl UdsClientPool {
    pub fn new() -> Self {
        Self {
            clients: std::collections::HashMap::new(),
        }
    }

    // プールからクライアントを取得または新規作成
    pub async fn get_client(&mut self, socket_path: &Path) -> Result<&mut UdsGrpcClient> {
        let socket_path = socket_path.to_path_buf();
        
        if !self.clients.contains_key(&socket_path) {
            let client = UdsGrpcClient::connect(&socket_path).await?;
            self.clients.insert(socket_path.clone(), client);
        }
        
        let client = self.clients.get_mut(&socket_path).unwrap();
        
        // 接続状態確認と再接続
        if !client.is_connected().await {
            client.reconnect().await?;
        }
        
        Ok(client)
    }

    // プールのクリーンアップ
    pub async fn cleanup(&mut self) {
        // 無効な接続を削除
        let mut to_remove = Vec::new();
        
        for (path, client) in &mut self.clients {
            if !client.is_connected().await {
                to_remove.push(path.clone());
            }
        }
        
        for path in to_remove {
            self.clients.remove(&path);
            debug!("Removed inactive client: {}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_client_creation_with_nonexistent_socket() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("nonexistent.sock");
        
        let result = UdsGrpcClient::connect(&socket_path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_parallel_client_empty_paths() {
        let results = ParallelUdsClient::get_multiple_status(vec![], 50).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_client_pool_creation() {
        let pool = UdsClientPool::new();
        assert_eq!(pool.clients.len(), 0);
    }
}