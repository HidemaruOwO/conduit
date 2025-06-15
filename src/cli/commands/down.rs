// downコマンドの実装

use crate::cli::DownArgs;
use crate::cli::commands::CommandResult;
use crate::common::error::Error;
use tracing::info;

pub async fn execute(args: DownArgs) -> CommandResult {
    info!("Stopping tunnels from configuration file: {}", args.file.display());
    
    println!("🛑 Stopping tunnels from {}", args.file.display());
    
    // TODO: プロセスレジストリから該当トンネルプロセスを検索・停止
    Err(Error::generic("Down command not yet implemented"))
}