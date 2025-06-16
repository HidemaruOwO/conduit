// versionコマンドの実装

use crate::cli::commands::CommandResult;

pub async fn execute() -> CommandResult {
    println!("conduit {}", env!("CARGO_PKG_VERSION"));
    println!("Build date: {}", env!("VERGEN_BUILD_DATE"));
    println!("Git commit: {}", env!("VERGEN_GIT_SHA"));
    println!("Rust version: {}", env!("VERGEN_RUSTC_SEMVER"));
    
    Ok(())
}