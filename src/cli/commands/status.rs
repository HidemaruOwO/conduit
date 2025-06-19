// statusコマンドの実装
// システム全体の状況表示、プロセス監視状況、Process Registry統計

use crate::cli::StatusArgs;
use crate::cli::commands::CommandResult;
use crate::common::error::Error;
use crate::registry::{ProcessRegistry, models::TunnelStatus};
use comfy_table::{Table, Cell, Color, Attribute};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;
use tracing::debug;

pub async fn execute(args: StatusArgs) -> CommandResult {
    debug!("Executing status command with format: {}", args.format);
    
    let status = collect_system_status().await?;
    
    match args.format.as_str() {
        "json" => output_json(&status)?,
        "yaml" => output_yaml(&status)?,
        "table" | _ => output_table(&status)?,
    }
    
    Ok(())
}

// システム全体の状況を収集
async fn collect_system_status() -> Result<SystemStatus, Error> {
    let mut status = SystemStatus::default();
    
    // Process Registry統計取得
    match ProcessRegistry::new(None).await {
        Ok(registry) => {
            status.registry_status = "Connected".to_string();
            
            if let Ok(tunnels) = registry.list_active_tunnels().await {
                status.total_tunnels = tunnels.len() as u32;
                status.active_tunnels = tunnels.iter()
                    .filter(|t| t.status == TunnelStatus::Running)
                    .count() as u32;
            }
        }
        Err(e) => {
            status.registry_status = format!("Error: {}", e);
        }
    }
    
    // UDSソケット数確認
    let sockets_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".config/conduit/sockets");
    
    if let Ok(mut entries) = fs::read_dir(&sockets_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if entry.path().extension().map_or(false, |ext| ext == "sock") {
                status.uds_sockets += 1;
            }
        }
    }
    
    // Conduitプロセス数
    #[cfg(unix)]
    {
        if let Ok(output) = tokio::process::Command::new("pgrep")
            .arg("-c")
            .arg("conduit")
            .output()
            .await
        {
            if let Ok(count_str) = String::from_utf8(output.stdout) {
                status.conduit_processes = count_str.trim().parse().unwrap_or(0);
            }
        }
    }
    
    status.timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    Ok(status)
}

// 表形式での出力
fn output_table(status: &SystemStatus) -> CommandResult {
    println!("📊 Conduit System Status");
    println!("═══════════════════════════════════════");
    
    let mut table = Table::new();
    table.set_header(vec![
        Cell::new("Component").add_attribute(Attribute::Bold).fg(Color::Blue),
        Cell::new("Status").add_attribute(Attribute::Bold).fg(Color::Blue),
        Cell::new("Count").add_attribute(Attribute::Bold).fg(Color::Blue),
    ]);
    
    // Registry状況
    let registry_color = if status.registry_status == "Connected" {
        Color::Green
    } else {
        Color::Red
    };
    
    table.add_row(vec![
        Cell::new("Process Registry"),
        Cell::new(&status.registry_status).fg(registry_color),
        Cell::new(&format!("{} tunnels", status.total_tunnels)),
    ]);
    
    table.add_row(vec![
        Cell::new("Active Tunnels"),
        Cell::new("Running").fg(Color::Green),
        Cell::new(&status.active_tunnels.to_string()),
    ]);
    
    table.add_row(vec![
        Cell::new("UDS Sockets"),
        Cell::new("Available").fg(Color::Green),
        Cell::new(&status.uds_sockets.to_string()),
    ]);
    
    table.add_row(vec![
        Cell::new("Conduit Processes"),
        Cell::new("Running").fg(Color::Green),
        Cell::new(&status.conduit_processes.to_string()),
    ]);
    
    println!("{}", table);
    
    // タイムスタンプ表示
    let timestamp_str = chrono::DateTime::from_timestamp(status.timestamp as i64, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    
    println!("\n⏰ Last updated: {}", timestamp_str);
    
    Ok(())
}

// JSON形式での出力
fn output_json(status: &SystemStatus) -> CommandResult {
    println!("{}", serde_json::to_string_pretty(status)?);
    Ok(())
}

// YAML形式での出力
fn output_yaml(status: &SystemStatus) -> CommandResult {
    println!("registry_status: {}", status.registry_status);
    println!("total_tunnels: {}", status.total_tunnels);
    println!("active_tunnels: {}", status.active_tunnels);
    println!("uds_sockets: {}", status.uds_sockets);
    println!("conduit_processes: {}", status.conduit_processes);
    println!("timestamp: {}", status.timestamp);
    Ok(())
}

// システムステータス構造体
#[derive(Debug, Clone, serde::Serialize, Default)]
struct SystemStatus {
    registry_status: String,
    total_tunnels: u32,
    active_tunnels: u32,
    uds_sockets: u32,
    conduit_processes: u32,
    timestamp: u64,
}