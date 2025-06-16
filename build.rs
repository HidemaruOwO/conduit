use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // vergen設定（既存のビルド情報生成）
    vergen::EmitBuilder::builder()
        .all_build()
        .all_cargo()
        .all_git()
        .all_rustc()
        .emit()?;

    // gRPC Protocol Buffersのコンパイル
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&out_dir)
        .compile(&["proto/tunnel.proto"], &["proto/"])?;

    // プロトコルファイルの変更監視
    println!("cargo:rerun-if-changed=proto/tunnel.proto");

    Ok(())
}