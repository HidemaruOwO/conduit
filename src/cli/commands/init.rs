//! Init command implementation
//! 
//! This command initializes keys and configuration for Conduit

use crate::cli::InitArgs;
use crate::cli::commands::CommandResult;
use crate::common::{config::Config, error::Error};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use std::fs;
use std::path::Path;
use tracing::{info, warn};

/// Execute the init command
pub async fn execute(args: InitArgs) -> CommandResult {
    let work_dir = args.directory.unwrap_or_else(|| std::env::current_dir().unwrap());
    
    info!("Initializing Conduit in directory: {}", work_dir.display());
    
    // Create necessary directories
    create_directories(&work_dir, args.force)?;
    
    // Generate key pair
    generate_keypair(&work_dir, args.force)?;
    
    // Create sample configuration file
    create_sample_config(&work_dir, args.force)?;
    
    println!("✅ Conduit initialization completed successfully!");
    println!("📁 Working directory: {}", work_dir.display());
    println!("🔑 Keys generated in: {}/keys/", work_dir.display());
    println!("⚙️  Sample configuration created: {}/conduit.toml", work_dir.display());
    println!("");
    println!("Next steps:");
    println!("1. Edit conduit.toml to configure your tunnels");
    println!("2. Start router: conduit router");
    println!("3. Start tunnels: conduit up");
    
    Ok(())
}

/// Create necessary directories
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

/// Generate Ed25519 key pair
fn generate_keypair(work_dir: &Path, force: bool) -> CommandResult {
    use ed25519_dalek::{SigningKey, VerifyingKey};
    use rand::rngs::OsRng;
    
    let private_key_path = work_dir.join("keys/client.key");
    let public_key_path = work_dir.join("keys/client.pub");
    
    // Check if keys already exist
    if (private_key_path.exists() || public_key_path.exists()) && !force {
        return Err(Error::config(
            "Key files already exist. Use --force to overwrite."
        ));
    }
    
    // Generate new key pair
    let mut csprng = OsRng {};
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key: VerifyingKey = signing_key.verifying_key();
    
    // Save private key
    let private_key_bytes = signing_key.to_bytes();
    let private_key_b64 = BASE64_STANDARD.encode(&private_key_bytes);
    fs::write(&private_key_path, private_key_b64)?;
    
    // Save public key
    let public_key_bytes = verifying_key.to_bytes();
    let public_key_b64 = BASE64_STANDARD.encode(&public_key_bytes);
    fs::write(&public_key_path, public_key_b64)?;
    
    // Set appropriate permissions (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        
        // Private key should be readable only by owner
        let mut perms = fs::metadata(&private_key_path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&private_key_path, perms)?;
        
        // Public key can be readable by others
        let mut perms = fs::metadata(&public_key_path)?.permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&public_key_path, perms)?;
    }
    
    info!("Generated ed25519 key pair");
    info!("Private key: {}", private_key_path.display());
    info!("Public key: {}", public_key_path.display());
    
    Ok(())
}

/// Create sample configuration file
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