// upコマンドの実装
// 設定ファイルベースのトンネル起動、Process Registry登録

use crate::cli::UpArgs;
use crate::cli::commands::CommandResult;
use crate::common::{config::Config, error::Error};
use crate::registry::ProcessRegistry;
use crate::registry::models::TunnelConfig;
use indicatif::{ProgressBar, ProgressStyle};
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info, error};
use uuid::Uuid;

pub async fn execute(args: UpArgs) -> CommandResult {
    debug!("Executing up command with config file: {}", args.file.display());
    
    // 設定ファイル読み込み
    let config = Config::from_file(&args.file)
        .map_err(|e| Error::generic(&format!("Failed to load config file: {}", e)))?;
    
    if config.tunnels.is_empty() {
        println!("No tunnels defined in configuration file.");
        return Ok(());
    }
    
    println!("🚀 Starting {} tunnel(s) from configuration...", config.tunnels.len());
    
    // Process Registry接続
    let registry = ProcessRegistry::new(None).await
        .map_err(|e| Error::generic(&format!("Failed to connect to registry: {}", e)))?;
    
    // プログレスバー設定
    let progress = ProgressBar::new(config.tunnels.len() as u64);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
            .unwrap()
            .progress_chars("##-")
    );
    
    let mut success_count = 0;
    let mut error_count = 0;
    let mut started_tunnels = Vec::new();
    
    // 各トンネルを順次起動
    for tunnel_config in &config.tunnels {
        progress.set_message(format!("Starting tunnel: {}", tunnel_config.name));
        
        match start_tunnel(&registry, tunnel_config, &config).await {
            Ok(tunnel_id) => {
                println!("✅ Started tunnel: {} -> {}", 
                    tunnel_config.name, tunnel_config.source);
                success_count += 1;
                started_tunnels.push((tunnel_id, tunnel_config.name.clone()));
            }
            Err(e) => {
                println!("❌ Failed to start tunnel {}: {}", tunnel_config.name, e);
                error!("Failed to start tunnel {}: {}", tunnel_config.name, e);
                error_count += 1;
            }
        }
        
        progress.inc(1);
    }
    
    progress.finish_with_message("Tunnel startup completed");
    
    // 結果サマリー表示
    println!("\n📊 Startup Summary:");
    println!("  ✅ Successfully started: {} tunnels", success_count);
    if error_count > 0 {
        println!("  ❌ Failed to start: {} tunnels", error_count);
    }
    
    if !started_tunnels.is_empty() {
        println!("\n🔗 Active Tunnels:");
        for (tunnel_id, name) in started_tunnels {
            println!("  {} (ID: {})", name, tunnel_id);
        }
        
        println!("\nTo stop all tunnels, run:");
        println!("  conduit down -f {}", args.file.display());
    }
    
    if error_count > 0 {
        Err(Error::generic("Some tunnels failed to start"))
    } else {
        Ok(())
    }
}

// 個別トンネル起動
async fn start_tunnel(
    registry: &ProcessRegistry,
    tunnel_config: &crate::common::config::TunnelConfig,
    config: &Config,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let tunnel_id = format!("{}-{}", tunnel_config.name, Uuid::new_v4().simple());
    
    debug!("Starting tunnel: {} (ID: {})", tunnel_config.name, tunnel_id);
    
    // Registry設定作成
    let registry_config = TunnelConfig {
        router_addr: format!("{}:{}", config.router.host, config.router.port),
        source_addr: tunnel_config.source.clone(),
        bind_addr: tunnel_config.bind.clone(),
        protocol: tunnel_config.protocol.clone(),
        timeout_seconds: 30,
        max_connections: 1000,
    };
    
    // Process Registryを使用してトンネルを作成・起動
    let pid = registry.create_and_start_tunnel(
        tunnel_id.clone(),
        tunnel_config.name.clone(),
        registry_config,
    ).await?;
    
    info!("Started tunnel process: {} (PID: {})", tunnel_config.name, pid);
    
    Ok(tunnel_id)
}