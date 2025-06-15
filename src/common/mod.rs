// アプリケーション全体で共有される共通機能
//
// Conduitアプリケーション全体で使用される共有型、ユーティリティ、機能を含む

pub mod config;
pub mod error;
pub mod types;

pub use error::{Error, Result};