// Conduitのエラーハンドリング
//
// アプリケーション全体のエラー型とResult型を定義

use thiserror::Error;

// アプリケーション全体のResult型
pub type Result<T> = std::result::Result<T, Error>;

// Conduitのメインエラー型
#[derive(Error, Debug)]
pub enum Error {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("Security error: {0}")]
    Security(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Tunnel error: {0}")]
    Tunnel(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Error: {0}")]
    Generic(String),
}

impl Error {
    pub fn config(msg: impl Into<String>) -> Self {
        Error::Config(msg.into())
    }

    pub fn network(msg: impl Into<String>) -> Self {
        Error::Network(msg.into())
    }

    pub fn tls(msg: impl Into<String>) -> Self {
        Error::Tls(msg.into())
    }

    pub fn security(msg: impl Into<String>) -> Self {
        Error::Security(msg.into())
    }

    pub fn authentication(msg: impl Into<String>) -> Self {
        Error::Authentication(msg.into())
    }

    pub fn tunnel(msg: impl Into<String>) -> Self {
        Error::Tunnel(msg.into())
    }

    pub fn protocol(msg: impl Into<String>) -> Self {
        Error::Protocol(msg.into())
    }

    pub fn generic(msg: impl Into<String>) -> Self {
        Error::Generic(msg.into())
    }
}

// anyhow::Errorからの変換
impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::Generic(err.to_string())
    }
}
