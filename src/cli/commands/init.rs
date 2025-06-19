// initコマンドの実装
// Ed25519キーペア生成、設定ディレクトリ作成、SQLiteデータベース初期化

use crate::cli::InitArgs;
use crate::cli::commands::CommandResult;
use crate::common::{config::Config, error::Error};
use crate::registry::{ProcessRegistry, sqlite::SqliteRegistry};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

pub async fn execute(args: InitArgs) -> CommandResult {
    let work_dir = args.directory.unwrap_or_else(|| std::env::current_dir().unwrap());
    
    info!("Initializing Conduit in directory: {}", work_dir.display());
    
    // architecture.mdの仕様に基づいて~/.config/conduit/ディレクトリを作成
    create_conduit_directories(args.force).await?;
    
    // プロジェクトディレクトリにkeysディレクトリを作成
    create_directories(&work_dir, args.force)?;
    generate_keypair(&work_dir, args.force)?;
    create_sample_config(&work_dir, args.force)?;
    
    // SQLiteデータベースの初期化
    initialize_sqlite_registry().await?;
    
    println!("✅ Conduit initialization completed successfully!");
    println!("📁 Working directory: {}", work_dir.display());
    println!("🔑 Keys generated in: {}/keys/", work_dir.display());
    println!("⚙️  Sample configuration created: {}/conduit.toml", work_dir.display());
    println!("🗃️  SQLite registry initialized: ~/.config/conduit/registry.db");
    println!("📂 Conduit directories created: ~/.config/conduit/");
    println!("");
    println!("Next steps:");
    println!("1. Edit conduit.toml to configure your tunnels");
    println!("2. Start router: conduit router");
    println!("3. Start tunnels: conduit up");
    
    Ok(())
}

// architecture.mdの仕様に従い~/.config/conduit/ディレクトリ構造を作成
async fn create_conduit_directories(force: bool) -> CommandResult {
    let conduit_dir = dirs::home_dir()
        .ok_or_else(|| Error::config("Could not determine home directory"))?
        .join(".config")
        .join("conduit");
    
    let sockets_dir = conduit_dir.join("sockets");
    let tunnels_dir = conduit_dir.join("tunnels");
    
    if conduit_dir.exists() && !force {
        info!("Conduit directories already exist: {}", conduit_dir.display());
    } else {
        fs::create_dir_all(&conduit_dir)?;
        fs::create_dir_all(&sockets_dir)?;
        fs::create_dir_all(&tunnels_dir)?;
        
        info!("Created Conduit directory structure: {}", conduit_dir.display());
        info!("Created sockets directory: {}", sockets_dir.display());
        info!("Created tunnels directory: {}", tunnels_dir.display());
        
        // セキュリティのためUnixでディレクトリ権限を適切に設定
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            
            let mut perms = fs::metadata(&conduit_dir)?.permissions();
            perms.set_mode(0o700); // ユーザーのみアクセス可能
            fs::set_permissions(&conduit_dir, perms)?;
            
            let mut perms = fs::metadata(&sockets_dir)?.permissions();
            perms.set_mode(0o700);
            fs::set_permissions(&sockets_dir, perms)?;
            
            let mut perms = fs::metadata(&tunnels_dir)?.permissions();
            perms.set_mode(0o700);
            fs::set_permissions(&tunnels_dir, perms)?;
        }
    }
    
    Ok(())
}

fn create_directories(work_dir: &Path, force: bool) -> CommandResult {
    let keys_dir = work_dir.join("keys");
    
    if keys_dir.exists() && !force {
        return Err(Error::config(
            "Keys directory already exists. Use --force to overwrite."
        ));
    }
    
    fs::create_dir_all(&keys_dir)?;
    info!("Created keys directory: {}", keys_dir.display());
    
    Ok(())
}

fn generate_keypair(work_dir: &Path, force: bool) -> CommandResult {
    use ed25519_dalek::{SigningKey, VerifyingKey};
    use rand::rngs::OsRng;
    
    let private_key_path = work_dir.join("keys/client.key");
    let public_key_path = work_dir.join("keys/client.pub");
    
    if (private_key_path.exists() || public_key_path.exists()) && !force {
        return Err(Error::config(
            "Key files already exist. Use --force to overwrite."
        ));
    }
    
    let mut csprng = OsRng {};
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key: VerifyingKey = signing_key.verifying_key();
    
    let private_key_bytes = signing_key.to_bytes();
    let private_key_b64 = BASE64_STANDARD.encode(&private_key_bytes);
    fs::write(&private_key_path, private_key_b64)?;
    
    let public_key_bytes = verifying_key.to_bytes();
    let public_key_b64 = BASE64_STANDARD.encode(&public_key_bytes);
    fs::write(&public_key_path, public_key_b64)?;
    
    // セキュリティのためUnixでファイル権限を適切に設定
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        
        let mut perms = fs::metadata(&private_key_path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&private_key_path, perms)?;
        
        let mut perms = fs::metadata(&public_key_path)?.permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&public_key_path, perms)?;
    }
    
    info!("Generated ed25519 key pair");
    info!("Private key: {}", private_key_path.display());
    info!("Public key: {}", public_key_path.display());
    
    Ok(())
}

// SQLite Registryの初期化
async fn initialize_sqlite_registry() -> CommandResult {
    let db_path = dirs::home_dir()
        .ok_or_else(|| Error::config("Could not determine home directory"))?
        .join(".config")
        .join("conduit")
        .join("registry.db");
    
    match ProcessRegistry::new(Some(db_path.clone())).await {
        Ok(_) => {
            info!("SQLite registry initialized successfully: {}", db_path.display());
            Ok(())
        }
        Err(e) => {
            warn!("Failed to initialize SQLite registry: {}", e);
            Err(Error::config(&format!("Failed to initialize SQLite registry: {}", e)))
        }
    }
}

fn create_sample_config(work_dir: &Path, force: bool) -> CommandResult {
    let config_path = work_dir.join("conduit.toml");
    
    if config_path.exists() && !force {
        warn!("Configuration file already exists: {}", config_path.display());
        return Ok(());
    }
    
    let sample_config = Config::sample();
    sample_config.to_file(&config_path)?;
    
    info!("Created sample configuration: {}", config_path.display());
    
    Ok(())
}