// Declare modules
mod database;
mod doc_loader;
mod embeddings;
mod error;
mod server;

// Use necessary items from modules and crates
use crate::{
    database::Database,
    embeddings::{EMBEDDING_CLIENT, EmbeddingConfig, initialize_embedding_provider},
    error::ServerError,
    server::RustDocsServer,
};
use serde::{Deserialize, Serialize};
use async_openai::{Client as OpenAIClient, config::OpenAIConfig};
use clap::Parser;
use std::env;
use rmcp::{
    transport::io::stdio,
    ServiceExt,
};

use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize)]
struct ProxyConfig {
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

#[derive(Parser, Debug)]
#[command(author, version, about = "Rust documentation MCP server using PostgreSQL vector database", long_about = None)]
struct Cli {
    /// The crate names to serve documentation for (space-separated)
    crate_names: Vec<String>,

    /// List all available crates in the database
    #[arg(short, long)]
    list: bool,

    /// Load all available crates from the database
    #[arg(short, long)]
    all: bool,

    /// Embedding provider to use (openai or voyage)
    #[arg(long, default_value = "openai")]
    embedding_provider: String,

    /// Embedding model to use
    #[arg(long)]
    embedding_model: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), ServerError> {
    // Load .env file if present
    dotenvy::dotenv().ok();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Initialize database connection
    eprintln!("🔌 Connecting to database...");
    let db = Database::new().await?;
    eprintln!("✅ Database connected successfully");

    // Handle list command
    if cli.list {
        let stats = db.get_crate_stats().await?;
        if stats.is_empty() {
            println!("No crates found in database.");
            println!("Use the 'populate_db' tool to add crates first:");
            println!("  cargo run --bin populate_db -- <crate_name>");
        } else {
            println!("{:<20} {:<15} {:<10} {:<10} {:<20}", "Crate", "Version", "Docs", "Tokens", "Last Updated");
            println!("{:-<80}", "");
            for stat in stats {
                println!(
                    "{:<20} {:<15} {:<10} {:<10} {:<20}",
                    stat.name,
                    stat.version.unwrap_or_else(|| "N/A".to_string()),
                    stat.total_docs,
                    stat.total_tokens,
                    stat.last_updated.format("%Y-%m-%d %H:%M")
                );
            }
        }
        return Ok(());
    }

    // Determine which crates to load
    let crate_names = if cli.all {
        eprintln!("Loading all available crates from database...");
        let stats = db.get_crate_stats().await?;
        if stats.is_empty() {
            eprintln!("No crates found in database. Use 'populate_db' to add some first.");
            return Ok(());
        }
        stats.into_iter().map(|stat| stat.name).collect()
    } else if cli.crate_names.is_empty() {
        eprintln!("Error: Please specify crate names or use --all to load all crates");
        eprintln!("Usage examples:");
        eprintln!("  cargo run --bin rustdocs_mcp_server -- anyhow tokio serde");
        eprintln!("  cargo run --bin rustdocs_mcp_server -- --all");
        eprintln!("  cargo run --bin rustdocs_mcp_server -- --list");
        return Err(ServerError::Config("No crate names specified".to_string()));
    } else {
        cli.crate_names
    };

    eprintln!("Target crates: {:?}", crate_names);

    // Check if all crates exist in database
    eprintln!("🔍 Checking if crates exist in database...");
    let mut missing_crates = Vec::new();
    for crate_name in &crate_names {
        eprintln!("  Checking: {}", crate_name);
        if !db.has_embeddings(crate_name).await? {
            missing_crates.push(crate_name.clone());
            eprintln!("  ❌ Missing: {}", crate_name);
        } else {
            eprintln!("  ✅ Found: {}", crate_name);
        }
    }

    if !missing_crates.is_empty() {
        eprintln!("Error: The following crates are not found in the database:");
        for crate_name in &missing_crates {
            eprintln!("  - {}", crate_name);
        }
        eprintln!("\nPlease populate them first using:");
        for crate_name in &missing_crates {
            eprintln!("  cargo run --bin populate_db -- --crate-name {}", crate_name);
        }
        eprintln!("\nOr see available crates with:");
        eprintln!("  cargo run --bin rustdocs_mcp_server -- --list");
        return Err(ServerError::Config(format!("Missing crates: {:?}", missing_crates)));
    }

    // Initialize embedding provider (needed for query embedding)
    let provider_name = cli.embedding_provider.to_lowercase();
    eprintln!("🤖 Initializing {} embedding provider...", provider_name);

    let embedding_config = match provider_name.as_str() {
        "openai" => {
            let model = cli.embedding_model.unwrap_or_else(|| "text-embedding-3-large".to_string());
            let openai_client = if let Ok(api_base) = env::var("OPENAI_API_BASE") {
                let config = OpenAIConfig::new().with_api_base(api_base);
                OpenAIClient::with_config(config)
            } else {
                OpenAIClient::new()
            };
            EmbeddingConfig::OpenAI {
                client: openai_client,
                model,
            }
        },
        "voyage" => {
            let api_key = env::var("VOYAGE_API_KEY")
                .map_err(|_| ServerError::MissingEnvVar("VOYAGE_API_KEY".to_string()))?;
            let model = cli.embedding_model.unwrap_or_else(|| "voyage-3.5".to_string());
            EmbeddingConfig::VoyageAI { api_key, model }
        },
        _ => {
            return Err(ServerError::Config(format!(
                "Unsupported embedding provider: {}. Use 'openai' or 'voyage'",
                provider_name
            )));
        }
    };

    let provider = initialize_embedding_provider(embedding_config);
    if EMBEDDING_CLIENT.set(provider).is_err() {
        return Err(ServerError::Internal("Failed to set embedding provider".to_string()));
    }
    eprintln!("✅ {} embedding provider initialized", provider_name);

    // Check for automatic backfill requirements
    if Path::new("proxy-config.json").exists() {
        eprintln!("📋 Checking proxy-config.json for automatic backfill requirements...");
        
        let config_content = fs::read_to_string("proxy-config.json")
            .map_err(|e| ServerError::Config(format!("Failed to read proxy-config.json: {}", e)))?;
        
        let config: ProxyConfig = serde_json::from_str(&config_content)
            .map_err(|e| ServerError::Config(format!("Failed to parse proxy-config.json: {}", e)))?;

        let mut needs_backfill = Vec::new();
        
        for crate_config in &config.crates {
            if !crate_config.enabled {
                continue;
            }
            
            // Only check crates that we're actually serving
            if !crate_names.contains(&crate_config.name) {
                continue;
            }
            
            if let Some(expected_docs) = crate_config.expected_docs {
                let current_count = db.count_crate_documents(&crate_config.name).await?;
                
                if current_count < expected_docs {
                    needs_backfill.push((
                        crate_config.name.clone(),
                        current_count,
                        expected_docs,
                        crate_config.features.clone(),
                    ));
                    eprintln!("  ⚠️  {}: {} docs in DB < {} expected", 
                        crate_config.name, current_count, expected_docs);
                } else {
                    eprintln!("  ✅ {}: {} docs in DB >= {} expected", 
                        crate_config.name, current_count, expected_docs);
                }
            }
        }

        if !needs_backfill.is_empty() {
            eprintln!("\n🔄 Automatic backfill required for {} crates:", needs_backfill.len());
            for (crate_name, current, expected, features) in &needs_backfill {
                eprintln!("  📦 {}: {} -> {} docs", crate_name, current, expected);
                if let Some(features) = features {
                    eprintln!("     Features: {:?}", features);
                }
            }
            
            eprintln!("\n💡 To trigger backfill, run:");
            for (crate_name, _, _, features) in &needs_backfill {
                if let Some(features) = features {
                    eprintln!("  cargo run --bin populate_db -- --crate-name {} --features {}", 
                        crate_name, features.join(","));
                } else {
                    eprintln!("  cargo run --bin populate_db -- --crate-name {}", crate_name);
                }
            }
            eprintln!("⚠️  Server will continue with current document counts");
        } else {
            eprintln!("✅ All crates have sufficient documentation in database");
        }
    }

    // Verify crates exist in database (no loading into memory)
    eprintln!("🔍 Verifying {} crates are available in database...", crate_names.len());
    let mut crate_stats = HashMap::new();
    
    for crate_name in &crate_names {
        let stats = db.get_crate_stats().await?;
        let crate_stat = stats.iter().find(|s| &s.name == crate_name);
        if let Some(stat) = crate_stat {
            crate_stats.insert(crate_name.clone(), stat.total_docs);
            eprintln!("  ✅ {}: {} documents available", crate_name, stat.total_docs);
        } else {
            eprintln!("  ❌ {}: not found in database", crate_name);
        }
    }

    let total_available_docs: i64 = crate_stats.values().map(|&v| v as i64).sum();
    
    eprintln!("\n📊 Database Summary:");
    eprintln!("  📚 Total available documents: {}", total_available_docs);
    eprintln!("  🗄️  Database-driven search (no memory loading)");

    let startup_message = if crate_names.len() == 1 {
        let doc_count = crate_stats.get(&crate_names[0]).unwrap_or(&0);
        format!(
            "Server for crate '{}' initialized. {} documents available via database search.",
            crate_names[0], doc_count
        )
    } else {
        let crate_summary: Vec<String> = crate_stats
            .iter()
            .map(|(name, count)| format!("{} ({})", name, count))
            .collect();
        format!(
            "Multi-crate server initialized. {} total documents available from {} crates: {}",
            total_available_docs,
            crate_names.len(),
            crate_summary.join(", ")
        )
    };

    eprintln!("\n✅ {}", startup_message);

    // Create the service instance (no documents/embeddings in memory)
    let combined_crate_name = if crate_names.len() == 1 {
        crate_names[0].clone()
    } else {
        format!("multi-crate[{}]", crate_names.join(","))
    };

    let service = RustDocsServer::new(
        combined_crate_name.clone(),
        vec![], // No documents in memory - use database search
        vec![], // No embeddings in memory - generate on demand
        db,
        startup_message,
    )?;

    eprintln!("Rust Docs MCP server starting via stdio...");

    // Serve the server using stdio transport
    let server_handle = service.serve(stdio()).await.map_err(|e| {
        eprintln!("Failed to start server: {:?}", e);
        ServerError::McpRuntime(e.to_string())
    })?;

    eprintln!("Rust Docs MCP server running for: {}", combined_crate_name);

    // Wait for the server to complete
    server_handle.waiting().await.map_err(|e| {
        eprintln!("Server encountered an error while running: {:?}", e);
        ServerError::McpRuntime(e.to_string())
    })?;

    eprintln!("Rust Docs MCP server stopped.");
    Ok(())
}
