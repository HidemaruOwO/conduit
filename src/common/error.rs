//! Error handling for Conduit
//!
//! This module defines the application-wide error types and result type.

use thiserror::Error;

/// Application-wide result type
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for Conduit
#[derive(Error, Debug)]
pub enum Error {
    /// Configuration-related errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Network-related errors
    #[error("Network error: {0}")]
    Network(String),

    /// TLS/Security-related errors
    #[error("TLS error: {0}")]
    Tls(String),

    /// Security-related errors
    #[error("Security error: {0}")]
    Security(String),

    /// Authentication errors
    #[error("Authentication error: {0}")]
    Authentication(String),

    /// Tunnel operation errors
    #[error("Tunnel error: {0}")]
    Tunnel(String),

    /// I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// TOML parsing errors
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    /// Generic errors
    #[error("Error: {0}")]
    Generic(String),
}

impl Error {
    /// Create a new configuration error
    pub fn config(msg: impl Into<String>) -> Self {
        Error::Config(msg.into())
    }

    /// Create a new network error
    pub fn network(msg: impl Into<String>) -> Self {
        Error::Network(msg.into())
    }

    /// Create a new TLS error
    pub fn tls(msg: impl Into<String>) -> Self {
        Error::Tls(msg.into())
    }

    /// Create a new security error
    pub fn security(msg: impl Into<String>) -> Self {
        Error::Security(msg.into())
    }

    /// Create a new authentication error
    pub fn authentication(msg: impl Into<String>) -> Self {
        Error::Authentication(msg.into())
    }

    /// Create a new tunnel error
    pub fn tunnel(msg: impl Into<String>) -> Self {
        Error::Tunnel(msg.into())
    }

    /// Create a new generic error
    pub fn generic(msg: impl Into<String>) -> Self {
        Error::Generic(msg.into())
    }
}

/// Convert from anyhow::Error
impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::Generic(err.to_string())
    }
}