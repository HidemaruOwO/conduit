//! Conduit - High-performance network tunneling software
//!
//! This library provides the core functionality for the Conduit network tunneling system.

pub mod cli;
pub mod client;
pub mod router;
pub mod common;

pub use common::{
    config::Config,
    error::{Error, Result},
};

/// Conduit library version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Conduit library name
pub const NAME: &str = env!("CARGO_PKG_NAME");

/// Conduit library description
pub const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");