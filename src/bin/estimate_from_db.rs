use rustdocs_mcp_server::{
    database::Database,
    error::ServerError,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Parser, Debug)]
#[command(author, version, about = "Set expected_docs based on current database counts", long_about = None)]
struct Cli {
    /// Multiplier for current database count (default: 1.2 for 20% buffer)
    #[arg(long, default_value_t = 1.2)]
    multiplier: f32,
}

#[derive(Debug, Deserialize, Serialize)]
struct ProxyConfig {
    rustdocs_binary_path: String,
    crates: Vec<CrateConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CrateConfig {
    name: String,
    features: Option<Vec<String>>,
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_docs: Option<usize>,
}

#[tokio::main]
async fn main() -> Result<(), ServerError> {
    let cli = Cli::parse();
    
    // Connect to database
    let db = Database::new().await?;
    
    // Load existing proxy config
    let config_path = "proxy-config.json";
    if !Path::new(config_path).exists() {
        return Err(ServerError::Config(format!("{} not found", config_path)));
    }
    
    let content = fs::read_to_string(config_path)
        .map_err(|e| ServerError::Config(format!("Failed to read {}: {}", config_path, e)))?;
    
    let mut config: ProxyConfig = serde_json::from_str(&content)
        .map_err(|e| ServerError::Config(format!("Failed to parse {}: {}", config_path, e)))?;

    // Get database stats
    let db_stats = db.get_crate_stats().await?;
    
    println!("🔍 Setting expected_docs based on current database counts (multiplier: {:.1})", cli.multiplier);
    
    let mut updated_count = 0;
    
    for crate_config in &mut config.crates {
        if !crate_config.enabled {
            continue;
        }
        
        if let Some(stat) = db_stats.iter().find(|s| s.name == crate_config.name) {
            let current_docs = stat.total_docs as usize;
            let expected_docs = ((current_docs as f32) * cli.multiplier).ceil() as usize;
            
            // Only update if we don't have expected_docs or if it's significantly different
            let should_update = match crate_config.expected_docs {
                None => true,
                Some(existing) => {
                    let ratio = existing as f32 / current_docs as f32;
                    ratio < 0.8 || ratio > 3.0 // Update if more than 20% under or 3x over
                }
            };
            
            if should_update {
                let old_expected = crate_config.expected_docs;
                crate_config.expected_docs = Some(expected_docs);
                updated_count += 1;
                
                match old_expected {
                    Some(old) => println!("  📝 {}: {} -> {} (DB: {})", 
                        crate_config.name, old, expected_docs, current_docs),
                    None => println!("  ➕ {}: {} (DB: {})", 
                        crate_config.name, expected_docs, current_docs),
                }
            } else {
                println!("  ✅ {}: {} (DB: {}) - no change needed", 
                    crate_config.name, crate_config.expected_docs.unwrap(), current_docs);
            }
        } else {
            println!("  ⚠️  {}: not found in database", crate_config.name);
        }
    }
    
    if updated_count > 0 {
        // Write updated config back to file
        let updated_content = serde_json::to_string_pretty(&config)
            .map_err(|e| ServerError::Config(format!("Failed to serialize config: {}", e)))?;
        
        fs::write(config_path, updated_content)
            .map_err(|e| ServerError::Config(format!("Failed to write {}: {}", config_path, e)))?;
        
        println!("\n✅ Updated {} crates in {}", updated_count, config_path);
    } else {
        println!("\n✅ No updates needed - all crates have reasonable expected_docs values");
    }
    
    Ok(())
}