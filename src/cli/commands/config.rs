// configコマンドの実装

use crate::cli::{ConfigArgs, ConfigAction};
use crate::cli::commands::CommandResult;
use crate::common::config::Config;
use crate::common::error::Error;
use std::path::PathBuf;
use tracing::info;

pub async fn execute(args: ConfigArgs) -> CommandResult {
    match args.action {
        ConfigAction::Show => show_config().await,
        ConfigAction::Validate { file } => validate_config(file).await,
        ConfigAction::Generate { output } => generate_config(output).await,
    }
}

async fn show_config() -> CommandResult {
    let config_path = PathBuf::from("conduit.toml");
    
    if !config_path.exists() {
        return Err(Error::config("Configuration file 'conduit.toml' not found. Run 'conduit init' first."));
    }
    
    let config = Config::from_file(&config_path)?;
    let toml_str = toml::to_string_pretty(&config)
        .map_err(|e| Error::config(format!("Failed to serialize config: {}", e)))?;
    
    println!("Current configuration (conduit.toml):");
    println!("{}", toml_str);
    
    Ok(())
}

async fn validate_config(file: Option<PathBuf>) -> CommandResult {
    let config_path = file.unwrap_or_else(|| PathBuf::from("conduit.toml"));
    
    if !config_path.exists() {
        return Err(Error::config(format!("Configuration file '{}' not found", config_path.display())));
    }
    
    info!("Validating configuration file: {}", config_path.display());
    
    match Config::from_file(&config_path) {
        Ok(config) => {
            println!("✅ Configuration is valid");
            println!("📊 Found {} tunnel(s) configured", config.tunnels.len());
            println!("🔗 Router: {}:{}", config.router.host, config.router.port);
            
            for tunnel in &config.tunnels {
                println!("   - {}: {} -> {}", tunnel.name, tunnel.bind, tunnel.source);
            }
        }
        Err(e) => {
            println!("❌ Configuration validation failed: {}", e);
            return Err(e);
        }
    }
    
    Ok(())
}

async fn generate_config(output: Option<PathBuf>) -> CommandResult {
    let output_path = output.unwrap_or_else(|| PathBuf::from("conduit-sample.toml"));
    
    if output_path.exists() {
        return Err(Error::config(format!("Output file '{}' already exists", output_path.display())));
    }
    
    let sample_config = Config::sample();
    sample_config.to_file(&output_path)?;
    
    println!("✅ Sample configuration generated: {}", output_path.display());
    println!("📝 Edit the file to customize your tunnels");
    
    Ok(())
}