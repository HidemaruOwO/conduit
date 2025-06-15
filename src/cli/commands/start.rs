// startコマンドの実装
//
// 単一のトンネル接続を開始

use crate::cli::StartArgs;
use crate::cli::commands::CommandResult;
use crate::common::error::Error;
use tracing::info;

pub async fn execute(args: StartArgs) -> CommandResult {
    info!("Starting single tunnel");
    info!("Router: {}", args.router);
    info!("Source: {}", args.source);
    info!("Bind: {}", args.bind);
    
    // TODO: デーモンレスアーキテクチャによるトンネルプロセス起動実装
    println!("🚀 Starting tunnel...");
    println!("📡 Router: {}", args.router);
    println!("🎯 Source: {}", args.source);
    println!("🔗 Bind: {}", args.bind);
    
    Err(Error::generic("Start command not yet implemented"))
}