// ConduitのCLIモジュール
//
// コマンドライン引数の解析とコマンド実行機能を提供

use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;

pub mod commands;

#[derive(Parser)]
#[command(
    name = "conduit",
    about = "High-performance network tunneling software",
    version = env!("CARGO_PKG_VERSION"),
    long_about = "Conduit is a high-performance network tunneling software that enables secure access to services in private networks through encrypted TLS connections."
)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize keys and configuration
    Init(InitArgs),
    
    /// Start a single tunnel
    Start(StartArgs),
    
    /// Start tunnels from configuration file
    Up(UpArgs),
    
    /// Stop tunnels started with 'up' command
    Down(DownArgs),
    
    /// Start router server
    Router(RouterArgs),
    
    /// List active tunnels and connections
    List(ListArgs),
    
    /// Kill specific tunnels or connections
    Kill(KillArgs),
    
    /// Show system status
    Status(StatusArgs),
    
    /// Manage configuration
    Config(ConfigArgs),
    
    /// Show version information
    Version,
}

#[derive(Parser)]
pub struct InitArgs {
    /// Directory to initialize (default: current directory)
    #[arg(short, long, value_name = "DIR")]
    pub directory: Option<PathBuf>,
    
    /// Force overwrite existing files
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Parser)]
pub struct StartArgs {
    /// Router address to connect to
    #[arg(short, long, value_name = "HOST:PORT")]
    pub router: SocketAddr,
    
    /// Source service address on router side
    #[arg(short, long, value_name = "HOST:PORT")]
    pub source: SocketAddr,
    
    /// Local bind address for incoming connections
    #[arg(short, long, value_name = "HOST:PORT")]
    pub bind: SocketAddr,
    
    /// Private key file path
    #[arg(short, long, value_name = "PATH")]
    pub key: Option<PathBuf>,
}

#[derive(Parser)]
pub struct UpArgs {
    /// Configuration file path
    #[arg(short, long, value_name = "FILE", default_value = "conduit.toml")]
    pub file: PathBuf,
    
    /// Run in daemon mode
    #[arg(short, long)]
    pub daemon: bool,
}

#[derive(Parser)]
pub struct DownArgs {
    /// Configuration file path
    #[arg(short, long, value_name = "FILE", default_value = "conduit.toml")]
    pub file: PathBuf,
}

#[derive(Parser)]
pub struct RouterArgs {
    /// Address to bind the router server
    #[arg(short, long, value_name = "HOST:PORT", default_value = "0.0.0.0:9999")]
    pub bind: SocketAddr,
    
    /// Private key file path
    #[arg(short, long, value_name = "PATH")]
    pub key: Option<PathBuf>,
    
    /// Run in daemon mode
    #[arg(short, long)]
    pub daemon: bool,
}

#[derive(Parser)]
pub struct ListArgs {
    /// Show only tunnels
    #[arg(short, long)]
    pub tunnels: bool,
    
    /// Show only connections
    #[arg(short, long)]
    pub connections: bool,
    
    /// Output format (table, json, yaml)
    #[arg(short, long, default_value = "table")]
    pub format: String,
}

#[derive(Parser)]
pub struct KillArgs {
    /// Kill all tunnels and connections
    #[arg(short, long)]
    pub all: bool,
    
    /// Tunnel name to kill
    #[arg(short, long, value_name = "NAME")]
    pub tunnel: Option<String>,
    
    /// Connection ID to kill
    #[arg(short, long, value_name = "ID")]
    pub connection: Option<String>,
}

#[derive(Parser)]
pub struct StatusArgs {
    /// Output format (table, json, yaml)
    #[arg(short, long, default_value = "table")]
    pub format: String,
    
    /// Show detailed information
    #[arg(short, long)]
    pub detailed: bool,
}

#[derive(Parser)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show current configuration
    Show,
    
    /// Validate configuration file
    Validate {
        /// Configuration file path
        #[arg(value_name = "FILE")]
        file: Option<PathBuf>,
    },
    
    /// Generate sample configuration
    Generate {
        /// Output file path
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
}