// listコマンドの実装
// Process Registryからトンネル一覧取得、表形式表示

use crate::cli::ListArgs;
use crate::cli::commands::CommandResult;
use crate::common::error::Error;
use crate::registry::{ProcessRegistry, models::{TunnelInfo, TunnelStatus}};
use comfy_table::{Table, Cell, Color, Attribute};
use serde_json::json;
use tracing::debug;

pub async fn execute(args: ListArgs) -> CommandResult {
    debug!("Executing list command with format: {}", args.format);
    
    // Process Registryからトンネル一覧を取得
    let registry = ProcessRegistry::new(None).await
        .map_err(|e| Error::generic(&format!("Failed to connect to registry: {}", e)))?;
    
    let tunnels = registry.list_active_tunnels().await
        .map_err(|e| Error::generic(&format!("Failed to list tunnels: {}", e)))?;
    
    if tunnels.is_empty() {
        println!("No active tunnels found.");
        return Ok(());
    }
    
    // フィルタリング適用
    let filtered_tunnels = apply_filters(tunnels, &args);
    
    // 出力形式に応じて表示
    match args.format.as_str() {
        "json" => output_json(&filtered_tunnels)?,
        "yaml" => output_yaml(&filtered_tunnels)?,
        "table" | _ => output_table(&filtered_tunnels, &args)?,
    }
    
    Ok(())
}

// フィルタリング適用
fn apply_filters(tunnels: Vec<TunnelInfo>, args: &ListArgs) -> Vec<TunnelInfo> {
    tunnels.into_iter()
        .filter(|tunnel| {
            if args.tunnels && args.connections {
                true // 両方指定された場合は全て表示
            } else if args.tunnels {
                true // トンネルのみ表示
            } else if args.connections {
                tunnel.status == TunnelStatus::Running // アクティブなもののみ
            } else {
                true // フィルタなしの場合は全て表示
            }
        })
        .collect()
}

// 表形式での出力
fn output_table(tunnels: &[TunnelInfo], args: &ListArgs) -> CommandResult {
    let mut table = Table::new();
    
    // ヘッダー設定
    if args.connections {
        table.set_header(vec![
            Cell::new("NAME").add_attribute(Attribute::Bold).fg(Color::Blue),
            Cell::new("STATUS").add_attribute(Attribute::Bold).fg(Color::Blue),
            Cell::new("PID").add_attribute(Attribute::Bold).fg(Color::Blue),
            Cell::new("SOCKET").add_attribute(Attribute::Bold).fg(Color::Blue),
            Cell::new("CREATED").add_attribute(Attribute::Bold).fg(Color::Blue),
        ]);
    } else {
        table.set_header(vec![
            Cell::new("NAME").add_attribute(Attribute::Bold).fg(Color::Blue),
            Cell::new("STATUS").add_attribute(Attribute::Bold).fg(Color::Blue),
            Cell::new("PID").add_attribute(Attribute::Bold).fg(Color::Blue),
            Cell::new("CREATED").add_attribute(Attribute::Bold).fg(Color::Blue),
        ]);
    }
    
    // データ行追加
    for tunnel in tunnels {
        let status_text = tunnel.status.as_str();
        
        let status_cell = match tunnel.status {
            TunnelStatus::Running => Cell::new(status_text).fg(Color::Green),
            TunnelStatus::Exited => Cell::new(status_text).fg(Color::Yellow),
            TunnelStatus::Error => Cell::new(status_text).fg(Color::Red),
            _ => Cell::new(status_text).fg(Color::White),
        };
        
        let created_str = format_timestamp(tunnel.created_at);
        let pid_str = tunnel.pid.map_or("N/A".to_string(), |p| p.to_string());
        
        if args.connections {
            table.add_row(vec![
                Cell::new(&tunnel.name),
                status_cell,
                Cell::new(&pid_str),
                Cell::new(&tunnel.socket_path.display().to_string()),
                Cell::new(&created_str),
            ]);
        } else {
            table.add_row(vec![
                Cell::new(&tunnel.name),
                status_cell,
                Cell::new(&pid_str),
                Cell::new(&created_str),
            ]);
        }
    }
    
    println!("{}", table);
    println!("\nTotal tunnels: {}", tunnels.len());
    
    Ok(())
}

// JSON形式での出力
fn output_json(tunnels: &[TunnelInfo]) -> CommandResult {
    let json_output = json!({
        "tunnels": tunnels,
        "total": tunnels.len()
    });
    
    println!("{}", serde_json::to_string_pretty(&json_output)?);
    Ok(())
}

// YAML形式での出力
fn output_yaml(tunnels: &[TunnelInfo]) -> CommandResult {
    println!("tunnels:");
    for tunnel in tunnels {
        println!("  - id: {}", tunnel.id);
        println!("    name: {}", tunnel.name);
        println!("    pid: {}", tunnel.pid.map_or("N/A".to_string(), |p| p.to_string()));
        println!("    status: {}", tunnel.status.as_str());
        println!("    socket_path: {}", tunnel.socket_path.display());
        println!("    created_at: {}", tunnel.created_at);
        println!();
    }
    println!("total: {}", tunnels.len());
    Ok(())
}

// タイムスタンプを人間が読みやすい形式に変換
fn format_timestamp(timestamp: i64) -> String {
    use chrono::{TimeZone, Utc};
    
    match Utc.timestamp_opt(timestamp, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        _ => "Invalid timestamp".to_string(),
    }
}