//! CLI command implementations
//!
//! This module contains the implementation of all CLI commands.

pub mod init;
pub mod start;
pub mod up;
pub mod down;
pub mod router;
pub mod list;
pub mod kill;
pub mod status;
pub mod config;
pub mod version;

use crate::common::error::Result;

/// Common result type for all command operations
pub type CommandResult = Result<()>;