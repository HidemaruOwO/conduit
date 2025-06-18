// gRPC over UDSプロトコル定義
// Protocol Buffersの生成コードとRustラッパー

use std::path::Path;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;
use anyhow::Result;

// Protocol Buffersで生成されるコードのインクルード
pub mod tunnel {
    tonic::include_proto!("tunnel");
}

pub use tunnel::{
    tunnel_control_server::{TunnelControl, TunnelControlServer},
    tunnel_control_client::TunnelControlClient,
    *
};

// UDS専用チャンネル作成ヘルパー
pub async fn create_uds_channel(socket_path: &Path) -> Result<Channel> {
    let socket_path = socket_path.to_owned();
    
    // UDS接続用のカスタムコネクタ
    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(move |_: Uri| {
            let socket_path = socket_path.clone();
            async move {
                let stream = tokio::net::UnixStream::connect(socket_path).await?;
                Ok::<_, std::io::Error>(stream)
            }
        }))
        .await?;
    
    Ok(channel)
}

// TunnelInfoの変換ヘルパー
impl From<crate::registry::models::TunnelInfo> for TunnelInfo {
    fn from(info: crate::registry::models::TunnelInfo) -> Self {
        Self {
            id: info.id,
            name: info.name,
            router_addr: info.config.router_addr,
            source_addr: info.config.source_addr,
            bind_addr: info.config.bind_addr,
            status: info.status as i32,
            created_at: info.created_at,
            updated_at: info.updated_at,
            pid: info.pid.unwrap_or(0) as i32,
            socket_path: info.socket_path.to_string_lossy().to_string(),
        }
    }
}

// TunnelMetricsの変換ヘルパー
impl From<crate::registry::models::TunnelMetrics> for TunnelMetrics {
    fn from(metrics: crate::registry::models::TunnelMetrics) -> Self {
        Self {
            active_connections: metrics.active_connections as i32,
            total_connections: metrics.total_connections as i64,
            total_bytes_sent: metrics.total_bytes_sent as i64,
            total_bytes_received: metrics.total_bytes_received as i64,
            cpu_usage: metrics.cpu_usage,
            memory_usage: metrics.memory_usage as i64,
            uptime_seconds: metrics.uptime_seconds as i64,
            avg_latency_ms: metrics.avg_latency_ms,
        }
    }
}

// ConnectionInfoの変換ヘルパー
impl From<crate::registry::models::ConnectionInfo> for ConnectionInfo {
    fn from(info: crate::registry::models::ConnectionInfo) -> Self {
        Self {
            id: info.id,
            client_addr: info.client_addr,
            target_addr: info.target_addr,
            connected_at: info.connected_at,
            bytes_sent: info.bytes_sent as i64,
            bytes_received: info.bytes_received as i64,
            status: info.status,
        }
    }
}

// レスポンス構築ヘルパー
pub mod response_builders {
    use super::*;
    use crate::registry::models;

    pub fn build_status_response(
        tunnel_info: models::TunnelInfo,
        connections: Vec<models::ConnectionInfo>,
    ) -> StatusResponse {
        let tunnel_info_proto = TunnelInfo::from(tunnel_info.clone());
        let connections_proto: Vec<ConnectionInfo> = connections
            .into_iter()
            .map(ConnectionInfo::from)
            .collect();
        let metrics_proto = TunnelMetrics::from(tunnel_info.metrics);

        StatusResponse {
            tunnel_info: Some(tunnel_info_proto),
            connections: connections_proto,
            metrics: Some(metrics_proto),
        }
    }

    pub fn build_list_response(connections: Vec<models::ConnectionInfo>) -> ListResponse {
        let connections_proto: Vec<ConnectionInfo> = connections
            .into_iter()
            .map(ConnectionInfo::from)
            .collect();

        ListResponse {
            connections: connections_proto,
        }
    }

    pub fn build_shutdown_response(success: bool, message: String) -> ShutdownResponse {
        ShutdownResponse { success, message }
    }

    pub fn build_metrics_response(metrics: models::TunnelMetrics, timestamp: i64) -> MetricsResponse {
        MetricsResponse {
            metrics: Some(TunnelMetrics::from(metrics)),
            timestamp,
        }
    }
}

// エラーハンドリング用のヘルパー
pub mod error_handling {
    use tonic::Status;
    use anyhow::Error;

    // anyhow::Errorをtonic::Statusに変換
    pub fn anyhow_to_status(err: Error) -> Status {
        // エラーの種類に応じてgrpcステータスコードを決定
        if err.to_string().contains("not found") {
            Status::not_found(err.to_string())
        } else if err.to_string().contains("permission") || err.to_string().contains("access") {
            Status::permission_denied(err.to_string())
        } else if err.to_string().contains("timeout") {
            Status::deadline_exceeded(err.to_string())
        } else if err.to_string().contains("connection") {
            Status::unavailable(err.to_string())
        } else {
            Status::internal(err.to_string())
        }
    }

    // 標準的なエラーレスポンス
    pub fn not_found_error(resource: &str) -> Status {
        Status::not_found(format!("{} not found", resource))
    }

    pub fn internal_error(message: &str) -> Status {
        Status::internal(message)
    }

    pub fn invalid_argument_error(message: &str) -> Status {
        Status::invalid_argument(message)
    }
}

// リクエスト検証用のヘルパー
pub mod validation {
    use super::*;
    use tonic::Status;

    pub fn validate_shutdown_request(request: &ShutdownRequest) -> Result<(), Status> {
        if request.timeout_seconds < 0 {
            return Err(Status::invalid_argument("Timeout must be non-negative"));
        }
        if request.timeout_seconds > 300 {
            return Err(Status::invalid_argument("Timeout cannot exceed 300 seconds"));
        }
        Ok(())
    }

    pub fn validate_tunnel_id(tunnel_id: &str) -> Result<(), Status> {
        if tunnel_id.is_empty() {
            return Err(Status::invalid_argument("Tunnel ID cannot be empty"));
        }
        if tunnel_id.len() > 100 {
            return Err(Status::invalid_argument("Tunnel ID too long"));
        }
        // 英数字、ハイフン、アンダースコアのみ許可
        if !tunnel_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(Status::invalid_argument("Invalid tunnel ID format"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::models;

    #[test]
    fn test_tunnel_info_conversion() {
        let tunnel_config = models::TunnelConfig {
            router_addr: "10.2.0.1:9999".to_string(),
            source_addr: "10.2.0.2:8080".to_string(),
            bind_addr: "0.0.0.0:80".to_string(),
            protocol: "tcp".to_string(),
            timeout_seconds: 30,
            max_connections: 100,
        };

        let tunnel_info = models::TunnelInfo {
            id: "test-id".to_string(),
            name: "test-tunnel".to_string(),
            pid: Some(12345),
            socket_path: std::path::PathBuf::from("/tmp/test.sock"),
            status: models::TunnelStatus::Running,
            config: tunnel_config,
            created_at: 1000000000,
            updated_at: 1000000001,
            last_activity: 1000000002,
            exit_code: None,
            metrics: models::TunnelMetrics::default(),
        };

        let proto_info = TunnelInfo::from(tunnel_info);
        assert_eq!(proto_info.id, "test-id");
        assert_eq!(proto_info.name, "test-tunnel");
        assert_eq!(proto_info.pid, 12345);
        assert_eq!(proto_info.status, 3); // Running = 3
    }

    #[test]
    fn test_validation() {
        use validation::*;

        // 有効なトンネルID
        assert!(validate_tunnel_id("valid-tunnel_123").is_ok());
        
        // 無効なトンネルID
        assert!(validate_tunnel_id("").is_err());
        assert!(validate_tunnel_id("invalid@tunnel").is_err());
        
        // 有効なシャットダウンリクエスト
        let valid_request = ShutdownRequest {
            force: false,
            timeout_seconds: 30,
        };
        assert!(validate_shutdown_request(&valid_request).is_ok());
        
        // 無効なシャットダウンリクエスト
        let invalid_request = ShutdownRequest {
            force: false,
            timeout_seconds: -1,
        };
        assert!(validate_shutdown_request(&invalid_request).is_err());
    }
}