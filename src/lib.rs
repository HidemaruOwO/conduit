// Conduit - 高性能ネットワークトンネリングソフトウェア
//
// Conduitネットワークトンネリングシステムのコア機能を提供するライブラリ

pub mod cli;
pub mod client;
pub mod router;
pub mod common;
pub mod protocol;
pub mod security;
pub mod registry;
pub mod ipc;

pub use common::{
    config::Config,
    error::{Error, Result},
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NAME: &str = env!("CARGO_PKG_NAME");
pub const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");