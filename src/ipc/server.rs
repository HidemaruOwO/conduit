// Unix Domain Socket gRPCサーバー
// Tunnel ProcessとCLI Commands間の通信サーバー

use crate::ipc::protocol::{self, tunnel::*};
use crate::registry::models::*;
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status, Streaming};
use tracing::{debug, error, info, warn};

// UDS gRPCサーバー実装
pub struct UdsGrpcServer {
    socket_path: std::path::PathBuf,
    tunnel_service: Arc<TunnelControlService>,
}

impl UdsGrpcServer {
    // 新しいUDS gRPCサーバーインスタンスの作成
    pub fn new(socket_path: &Path, tunnel_id: String) -> Result<Self> {
        let tunnel_service = Arc::new(TunnelControlService::new(tunnel_id)?);
        
        Ok(Self {
            socket_path: socket_path.to_path_buf(),
            tunnel_service,
        })
    }

    // サーバーの開始
    pub async fn serve(&self) -> Result<()> {
        info!("Starting UDS gRPC server at: {}", self.socket_path.display());

        // 既存のソケットファイルを削除
        if self.socket_path.exists() {
            tokio::fs::remove_file(&self.socket_path).await?;
        }

        // Unix Domain Socketリスナーの作成
        let listener = tokio::net::UnixListener::bind(&self.socket_path)?;

        // ソケットファイルの権限設定（所有者のみアクセス可能）
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.socket_path, perms)?;
        }

        let incoming = tokio_stream::wrappers::UnixListenerStream::new(listener);

        // gRPCサーバーの設定と起動
        Server::builder()
            .add_service(TunnelControlServer::new(Arc::clone(&self.tunnel_service)))
            .serve_with_incoming(incoming)
            .await?;

        Ok(())
    }

    // サーバーの停止とクリーンアップ
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down UDS gRPC server");
        
        // ソケットファイルの削除
        if self.socket_path.exists() {
            tokio::fs::remove_file(&self.socket_path).await?;
            debug!("Cleaned up socket file: {}", self.socket_path.display());
        }
        
        Ok(())
    }
}

// TunnelControl gRPCサービス実装
pub struct TunnelControlService {
    tunnel_id: String,
    tunnel_info: Arc<RwLock<TunnelInfo>>,
    connections: Arc<RwLock<Vec<ConnectionInfo>>>,
    metrics: Arc<RwLock<TunnelMetrics>>,
    shutdown_signal: Arc<RwLock<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl TunnelControlService {
    pub fn new(tunnel_id: String) -> Result<Self> {
        // 初期のトンネル情報を設定
        let tunnel_info = TunnelInfo {
            id: tunnel_id.clone(),
            name: "unknown".to_string(),
            pid: Some(std::process::id()),
            socket_path: std::path::PathBuf::new(),
            status: TunnelStatus::Created,
            config: TunnelConfig {
                router_addr: "unknown".to_string(),
                source_addr: "unknown".to_string(),
                bind_addr: "unknown".to_string(),
                protocol: "tcp".to_string(),
                timeout_seconds: 30,
                max_connections: 100,
            },
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
            last_activity: chrono::Utc::now().timestamp(),
            exit_code: None,
            metrics: TunnelMetrics::default(),
        };

        Ok(Self {
            tunnel_id,
            tunnel_info: Arc::new(RwLock::new(tunnel_info)),
            connections: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(RwLock::new(TunnelMetrics::default())),
            shutdown_signal: Arc::new(RwLock::new(None)),
        })
    }

    // トンネル情報の更新
    pub async fn update_tunnel_info(&self, info: TunnelInfo) {
        let mut tunnel_info = self.tunnel_info.write().await;
        *tunnel_info = info;
    }

    // 接続情報の更新
    pub async fn update_connections(&self, connections: Vec<ConnectionInfo>) {
        let mut conn_guard = self.connections.write().await;
        *conn_guard = connections;
    }

    // メトリクスの更新
    pub async fn update_metrics(&self, metrics: TunnelMetrics) {
        let mut metrics_guard = self.metrics.write().await;
        *metrics_guard = metrics;
    }
}

#[tonic::async_trait]
impl TunnelControl for TunnelControlService {
    // トンネル状態取得
    async fn get_status(
        &self,
        _request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        debug!("Received GetStatus request for tunnel: {}", self.tunnel_id);

        let tunnel_info = self.tunnel_info.read().await.clone();
        let connections = self.connections.read().await.clone();

        let response = protocol::response_builders::build_status_response(tunnel_info, connections);

        Ok(Response::new(response))
    }

    // アクティブ接続一覧取得
    async fn list_connections(
        &self,
        _request: Request<ListRequest>,
    ) -> Result<Response<ListResponse>, Status> {
        debug!("Received ListConnections request for tunnel: {}", self.tunnel_id);

        let connections = self.connections.read().await.clone();
        let response = protocol::response_builders::build_list_response(connections);

        Ok(Response::new(response))
    }

    // トンネル停止
    async fn shutdown(
        &self,
        request: Request<ShutdownRequest>,
    ) -> Result<Response<ShutdownResponse>, Status> {
        let req = request.into_inner();
        info!("Received Shutdown request for tunnel: {} (force: {})", self.tunnel_id, req.force);

        // リクエスト検証
        protocol::validation::validate_shutdown_request(&req)
            .map_err(|e| {
                warn!("Invalid shutdown request: {}", e);
                e
            })?;

        // シャットダウンシグナルの送信
        let mut shutdown_guard = self.shutdown_signal.write().await;
        if let Some(sender) = shutdown_guard.take() {
            match sender.send(()) {
                Ok(_) => {
                    let response = protocol::response_builders::build_shutdown_response(
                        true,
                        "Shutdown initiated successfully".to_string(),
                    );
                    info!("Tunnel {} shutdown initiated", self.tunnel_id);
                    Ok(Response::new(response))
                }
                Err(_) => {
                    let response = protocol::response_builders::build_shutdown_response(
                        false,
                        "Failed to send shutdown signal".to_string(),
                    );
                    error!("Failed to send shutdown signal for tunnel: {}", self.tunnel_id);
                    Ok(Response::new(response))
                }
            }
        } else {
            let response = protocol::response_builders::build_shutdown_response(
                false,
                "Shutdown signal not available".to_string(),
            );
            warn!("Shutdown signal not available for tunnel: {}", self.tunnel_id);
            Ok(Response::new(response))
        }
    }

    // メトリクスストリーミング
    type GetMetricsStreamStream = ReceiverStream<Result<MetricsResponse, Status>>;

    async fn get_metrics_stream(
        &self,
        _request: Request<MetricsRequest>,
    ) -> Result<Response<Self::GetMetricsStreamStream>, Status> {
        debug!("Received GetMetricsStream request for tunnel: {}", self.tunnel_id);

        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let metrics = Arc::clone(&self.metrics);
        let tunnel_id = self.tunnel_id.clone();

        // メトリクスストリーミングタスクの開始
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            
            loop {
                interval.tick().await;
                
                let current_metrics = metrics.read().await.clone();
                let timestamp = chrono::Utc::now().timestamp();
                let response = protocol::response_builders::build_metrics_response(
                    current_metrics,
                    timestamp,
                );
                
                if tx.send(Ok(response)).await.is_err() {
                    debug!("Metrics stream client disconnected for tunnel: {}", tunnel_id);
                    break;
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

// シャットダウンシグナル設定用のヘルパー
impl TunnelControlService {
    pub async fn set_shutdown_signal(&self, sender: tokio::sync::oneshot::Sender<()>) {
        let mut shutdown_guard = self.shutdown_signal.write().await;
        *shutdown_guard = Some(sender);
    }

    // ping機能（ヘルスチェック用）
    pub async fn ping(&self) -> Result<(), Status> {
        debug!("Ping request for tunnel: {}", self.tunnel_id);
        Ok(())
    }
}

// Tunnel Process内でのUDS gRPCサーバー管理用の統合構造体
pub struct TunnelProcessServer {
    server: UdsGrpcServer,
    service: Arc<TunnelControlService>,
    shutdown_rx: Option<tokio::sync::oneshot::Receiver<()>>,
}

impl TunnelProcessServer {
    pub async fn new(socket_path: &Path, tunnel_id: String) -> Result<Self> {
        let server = UdsGrpcServer::new(socket_path, tunnel_id.clone())?;
        let service = Arc::clone(&server.tunnel_service);
        
        // シャットダウンシグナルの設定
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        service.set_shutdown_signal(shutdown_tx).await;

        Ok(Self {
            server,
            service,
            shutdown_rx: Some(shutdown_rx),
        })
    }

    // サーバーの開始（シャットダウンシグナル付き）
    pub async fn serve_with_shutdown(&mut self) -> Result<()> {
        let shutdown_rx = self.shutdown_rx.take()
            .ok_or_else(|| anyhow::anyhow!("Shutdown receiver already consumed"))?;

        tokio::select! {
            result = self.server.serve() => {
                error!("gRPC server exited unexpectedly: {:?}", result);
                result
            }
            _ = shutdown_rx => {
                info!("Shutdown signal received, stopping gRPC server");
                self.server.shutdown().await?;
                Ok(())
            }
        }
    }

    // サービス参照の取得
    pub fn get_service(&self) -> Arc<TunnelControlService> {
        Arc::clone(&self.service)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_tunnel_control_service_creation() {
        let service = TunnelControlService::new("test-tunnel".to_string()).unwrap();
        
        // ping機能のテスト
        assert!(service.ping().await.is_ok());
    }

    #[tokio::test] 
    async fn test_uds_grpc_server_creation() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("test.sock");
        
        let server = UdsGrpcServer::new(&socket_path, "test-tunnel".to_string()).unwrap();
        assert_eq!(server.socket_path, socket_path);
    }

    #[tokio::test]
    async fn test_service_data_updates() {
        let service = TunnelControlService::new("test-tunnel".to_string()).unwrap();
        
        // メトリクスの更新テスト
        let new_metrics = TunnelMetrics {
            active_connections: 5,
            total_connections: 100,
            ..Default::default()
        };
        
        service.update_metrics(new_metrics.clone()).await;
        
        let stored_metrics = service.metrics.read().await.clone();
        assert_eq!(stored_metrics.active_connections, 5);
        assert_eq!(stored_metrics.total_connections, 100);
    }
}