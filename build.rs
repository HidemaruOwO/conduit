//! Build script for Conduit
//! 
//! This script generates build-time information using vergen

use vergen::EmitBuilder;

fn main() {
    // Generate build-time environment variables
    if let Err(error) = EmitBuilder::builder()
        .all_build()
        .all_cargo()
        .all_git()
        .all_rustc()
        .emit()
    {
        eprintln!("Failed to generate build info: {}", error);
        std::process::exit(1);
    }
}