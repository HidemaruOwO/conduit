// IPC統合管理
// Unix Domain Socket gRPC通信システム

pub mod server;
pub mod client;
pub mod protocol;

pub use server::UdsGrpcServer;
pub use client::UdsGrpcClient;
pub use protocol::*;

use anyhow::Result;
use std::path::Path;
use tracing::{debug, info};

// UDS通信システムの初期化
pub async fn init_uds_system() -> Result<()> {
    info!("Initializing UDS communication system");
    
    // ソケットディレクトリの作成
    let socket_dir = get_socket_directory()?;
    tokio::fs::create_dir_all(&socket_dir).await?;
    
    // Unix系でのディレクトリ権限設定
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o700);
        std::fs::set_permissions(&socket_dir, perms)?;
    }
    
    debug!("UDS socket directory prepared: {}", socket_dir.display());
    Ok(())
}

// ソケットディレクトリのパス取得
pub fn get_socket_directory() -> Result<std::path::PathBuf> {
    Ok(dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".conduit")
        .join("sockets"))
}

// トンネル用ソケットパスの生成
pub fn get_tunnel_socket_path(tunnel_id: &str) -> Result<std::path::PathBuf> {
    Ok(get_socket_directory()?.join(format!("{}.sock", tunnel_id)))
}

// ソケットファイルのクリーンアップ
pub async fn cleanup_socket_file(socket_path: &Path) -> Result<()> {
    if socket_path.exists() {
        tokio::fs::remove_file(socket_path).await?;
        debug!("Cleaned up socket file: {}", socket_path.display());
    }
    Ok(())
}

// UDS接続のヘルスチェック
pub async fn health_check_uds_connection(socket_path: &Path) -> Result<bool> {
    match UdsGrpcClient::connect(socket_path).await {
        Ok(mut client) => {
            // 基本的な接続テスト
            match client.ping().await {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        }
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[tokio::test]
    async fn test_socket_directory_creation() {
        let result = get_socket_directory();
        assert!(result.is_ok());
        
        let socket_dir = result.unwrap();
        assert!(socket_dir.to_string_lossy().contains(".conduit"));
    }
    
    #[test]
    fn test_tunnel_socket_path_generation() {
        let socket_path = get_tunnel_socket_path("test-tunnel").unwrap();
        assert!(socket_path.to_string_lossy().contains("test-tunnel.sock"));
    }
}