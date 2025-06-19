// downコマンドの実装
// 設定ファイルのトンネル停止、リソースクリーンアップ

use crate::cli::DownArgs;
use crate::cli::commands::CommandResult;
use crate::common::{config::Config, error::Error};
use crate::registry::ProcessRegistry;
use dialoguer::Confirm;
use tracing::{debug, info};

pub async fn execute(args: DownArgs) -> CommandResult {
    debug!("Executing down command with config file: {}", args.file.display());
    
    // 設定ファイル読み込み
    let config = Config::from_file(&args.file)
        .map_err(|e| Error::generic(&format!("Failed to load config file: {}", e)))?;
    
    if config.tunnels.is_empty() {
        println!("No tunnels defined in configuration file.");
        return Ok(());
    }
    
    // Process Registry接続
    let registry = ProcessRegistry::new(None).await
        .map_err(|e| Error::generic(&format!("Failed to connect to registry: {}", e)))?;
    
    // アクティブなトンネル一覧取得
    let active_tunnels = registry.list_active_tunnels().await
        .map_err(|e| Error::generic(&format!("Failed to list active tunnels: {}", e)))?;
    
    // 設定ファイルで定義されたトンネル名にマッチするものを抽出
    let config_tunnel_names: std::collections::HashSet<_> = config.tunnels.iter()
        .map(|t| &t.name)
        .collect();
    
    let matching_tunnels: Vec<_> = active_tunnels.into_iter()
        .filter(|t| config_tunnel_names.contains(&t.name))
        .collect();
    
    if matching_tunnels.is_empty() {
        println!("No active tunnels found matching configuration file.");
        return Ok(());
    }
    
    println!("🛑 Found {} tunnel(s) to stop:", matching_tunnels.len());
    for tunnel in &matching_tunnels {
        println!("  - {} (ID: {})", tunnel.name, tunnel.id);
    }
    
    // 安全性チェック（確認プロンプト）
    let confirmation = Confirm::new()
        .with_prompt("Are you sure you want to stop these tunnels?")
        .default(false)
        .interact()
        .map_err(|e| Error::generic(&format!("Failed to get user confirmation: {}", e)))?;
    
    if !confirmation {
        println!("Operation cancelled.");
        return Ok(());
    }
    
    let mut stopped_count = 0;
    let mut error_count = 0;
    
    // 各トンネルを停止
    for tunnel in matching_tunnels {
        println!("🔄 Stopping tunnel: {}", tunnel.name);
        
        match registry.stop_tunnel(&tunnel.id, false).await {
            Ok(true) => {
                println!("✅ Stopped tunnel: {}", tunnel.name);
                info!("Stopped tunnel: {} ({})", tunnel.name, tunnel.id);
                stopped_count += 1;
            }
            Ok(false) => {
                println!("⚠️  Tunnel {} was already stopped", tunnel.name);
                stopped_count += 1;
            }
            Err(e) => {
                println!("❌ Failed to stop tunnel {}: {}", tunnel.name, e);
                error_count += 1;
            }
        }
    }
    
    // 結果サマリー表示
    println!("\n📊 Shutdown Summary:");
    println!("  ✅ Successfully stopped: {} tunnels", stopped_count);
    if error_count > 0 {
        println!("  ❌ Failed to stop: {} tunnels", error_count);
    }
    
    // デッドプロセスのクリーンアップ
    println!("\n🧹 Cleaning up dead processes...");
    match registry.cleanup_dead_processes().await {
        Ok(cleaned) => {
            if !cleaned.is_empty() {
                println!("✅ Cleaned up {} dead process(es)", cleaned.len());
            } else {
                println!("✅ No dead processes found");
            }
        }
        Err(e) => {
            println!("⚠️  Failed to cleanup dead processes: {}", e);
        }
    }
    
    if error_count > 0 {
        Err(Error::generic("Some tunnels failed to stop"))
    } else {
        println!("\n🎉 All tunnels from configuration stopped successfully");
        Ok(())
    }
}