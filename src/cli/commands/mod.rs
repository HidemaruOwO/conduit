// CLIコマンド実装モジュール
//
// 全CLIコマンドの実装を含むモジュール

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

pub type CommandResult = Result<()>;