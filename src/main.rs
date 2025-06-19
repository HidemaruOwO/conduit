// Conduit - 高性能ネットワークトンネリングソフトウェア
//
// ConduitのCLIアプリケーションのメインエントリポイント

use clap::Parser;
use std::process;
use tracing::{error, info};
use conduit::cli::{CliArgs, Commands};

#[tokio::main]
async fn main() {
    init_logging();
    
    let args = CliArgs::parse();
    
    let result = match args.command {
        Commands::Init(cmd) => conduit::cli::commands::init::execute(cmd).await,
        Commands::Start(cmd) => conduit::cli::commands::start::execute(cmd).await,
        Commands::Up(cmd) => conduit::cli::commands::up::execute(cmd).await,
        Commands::Down(cmd) => conduit::cli::commands::down::execute(cmd).await,
        Commands::Router(cmd) => conduit::cli::commands::router::execute(cmd).await,
        Commands::List(cmd) => conduit::cli::commands::list::execute(cmd).await,
        Commands::Kill(cmd) => conduit::cli::commands::kill::execute(cmd).await,
        Commands::Status(cmd) => conduit::cli::commands::status::execute(cmd).await,
        Commands::Config(cmd) => conduit::cli::commands::config::execute(cmd).await,
        Commands::Version => conduit::cli::commands::version::execute().await,
    };

    if let Err(e) = result {
        error!("Command execution failed: {}", e);
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("conduit=info".parse().unwrap())
        )
        .init();
    
    info!("Conduit starting up");
}