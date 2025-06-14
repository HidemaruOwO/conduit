//! Conduit - High-performance network tunneling software
//!
//! This is the main entry point for the Conduit CLI application.

use clap::{Parser, Subcommand};
use std::process;
use tracing::{error, info};

mod cli;
mod common;

use cli::{CliArgs, Commands};

#[tokio::main]
async fn main() {
    // Initialize logging
    init_logging();
    
    let args = CliArgs::parse();
    
    // Execute subcommand
    let result = match args.command {
        Commands::Init(cmd) => cli::commands::init::execute(cmd).await,
        Commands::Start(cmd) => cli::commands::start::execute(cmd).await,
        Commands::Up(cmd) => cli::commands::up::execute(cmd).await,
        Commands::Down(cmd) => cli::commands::down::execute(cmd).await,
        Commands::Router(cmd) => cli::commands::router::execute(cmd).await,
        Commands::List(cmd) => cli::commands::list::execute(cmd).await,
        Commands::Kill(cmd) => cli::commands::kill::execute(cmd).await,
        Commands::Status(cmd) => cli::commands::status::execute(cmd).await,
        Commands::Config(cmd) => cli::commands::config::execute(cmd).await,
        Commands::Version => cli::commands::version::execute().await,
    };

    // Error handling
    if let Err(e) = result {
        error!("Command execution failed: {}", e);
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

/// Initialize the logging system
fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("conduit=info".parse().unwrap())
        )
        .init();
    
    info!("Conduit starting up");
}