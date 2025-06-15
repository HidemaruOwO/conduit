// listコマンドの実装

use crate::cli::ListArgs;
use crate::cli::commands::CommandResult;
use crate::common::error::Error;

pub async fn execute(args: ListArgs) -> CommandResult {
    println!("📋 Listing active tunnels and connections");
    println!("Format: {}", args.format);
    
    if args.tunnels {
        println!("Tunnels only");
    } else if args.connections {
        println!("Connections only");
    } else {
        println!("All tunnels and connections");
    }
    
    // TODO: プロセスレジストリからトンネルプロセス情報をgRPC経由で取得
    Err(Error::generic("List command not yet implemented"))
}