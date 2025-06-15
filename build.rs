//! Conduitのビルドスクリプト
//!
//! vergenを使用してビルド時情報を生成

use vergen::EmitBuilder;

fn main() {
    // ビルド時環境変数を生成（バージョン情報用）
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