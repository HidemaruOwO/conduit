// killコマンドの実装
// Process Manager経由でのトンネル終了、Graceful/Forceful終了オプション

use crate::cli::KillArgs;
use crate::cli::commands::CommandResult;
use crate::common::error::Error;
use crate::registry::ProcessRegistry;
use dialoguer::Confirm;
use tracing::{debug, info, warn};

pub async fn execute(args: KillArgs) -> CommandResult {
    debug!("Executing kill command");
    
    let registry = ProcessRegistry::new(None).await
        .map_err(|e| Error::generic(&format!("Failed to connect to registry: {}", e)))?;
    
    if args.all {
        kill_all_tunnels(&registry).await
    } else if let Some(tunnel_name) = args.tunnel {
        kill_tunnel_by_name(&registry, &tunnel_name).await
    } else if let Some(connection_id) = args.connection {
        kill_connection(&registry, &connection_id).await
    } else {
        Err(Error::generic("Please specify --all, --tunnel <name>, or --connection <id>"))
    }
}

// 全トンネル終了
async fn kill_all_tunnels(registry: &ProcessRegistry) -> CommandResult {
    let tunnels = registry.list_active_tunnels().await
        .map_err(|e| Error::generic(&format!("Failed to list tunnels: {}", e)))?;
    
    if tunnels.is_empty() {
        println!("No active tunnels found.");
        return Ok(());
    }
    
    // 安全性チェック（確認プロンプト）
    let confirmation = Confirm::new()
        .with_prompt(&format!("Are you sure you want to kill {} active tunnel(s)?", tunnels.len()))
        .default(false)
        .interact()
        .map_err(|e| Error::generic(&format!("Failed to get user confirmation: {}", e)))?;
    
    if !confirmation {
        println!("Operation cancelled.");
        return Ok(());
    }
    
    // Process Registryの一括停止機能を使用
    match registry.stop_all_tunnels(true).await {
        Ok(stopped_tunnels) => {
            println!("✅ Successfully killed {} tunnel(s)", stopped_tunnels.len());
            for tunnel_id in stopped_tunnels {
                println!("  - {}", tunnel_id);
            }
            info!("Killed all tunnels");
        }
        Err(e) => {
            return Err(Error::generic(&format!("Failed to kill tunnels: {}", e)));
        }
    }
    
    Ok(())
}

// 名前指定でトンネル終了
async fn kill_tunnel_by_name(registry: &ProcessRegistry, tunnel_name: &str) -> CommandResult {
    let tunnels = registry.list_active_tunnels().await
        .map_err(|e| Error::generic(&format!("Failed to list tunnels: {}", e)))?;
    
    let tunnel = tunnels.iter().find(|t| t.name == tunnel_name)
        .ok_or_else(|| Error::generic(&format!("Tunnel '{}' not found", tunnel_name)))?;
    
    // 安全性チェック（確認プロンプト）
    let confirmation = Confirm::new()
        .with_prompt(&format!("Are you sure you want to kill tunnel '{}'?", tunnel_name))
        .default(false)
        .interact()
        .map_err(|e| Error::generic(&format!("Failed to get user confirmation: {}", e)))?;
    
    if !confirmation {
        println!("Operation cancelled.");
        return Ok(());
    }
    
    match registry.stop_tunnel(&tunnel.id, true).await {
        Ok(true) => {
            println!("✅ Successfully killed tunnel: {} (PID: {})", tunnel_name,
                tunnel.pid.map_or("N/A".to_string(), |p| p.to_string()));
            info!("Killed tunnel: {} ({})", tunnel_name, tunnel.id);
        }
        Ok(false) => {
            println!("⚠️  Tunnel '{}' was already stopped", tunnel_name);
        }
        Err(e) => {
            return Err(Error::generic(&format!("Failed to kill tunnel '{}': {}", tunnel_name, e)));
        }
    }
    
    Ok(())
}

// 接続ID指定で終了（簡易実装）
async fn kill_connection(_registry: &ProcessRegistry, connection_id: &str) -> CommandResult {
    // TODO: 実際の接続管理実装時に詳細化
    warn!("Connection killing not yet implemented for ID: {}", connection_id);
    println!("⚠️  Connection killing feature is not yet implemented.");
    println!("Connection ID: {}", connection_id);
    Ok(())
}