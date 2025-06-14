//! Common functionality shared across the application
//!
//! This module contains shared types, utilities, and functionality
//! used throughout the Conduit application.

pub mod config;
pub mod error;
pub mod types;

pub use error::{Error, Result};