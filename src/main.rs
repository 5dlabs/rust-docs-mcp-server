// Declare modules
mod database;
mod doc_loader;
mod embeddings;
mod error;
mod server;

// Use necessary items from modules and crates
use crate::{
    database::Database,
    doc_loader::Document,
    embeddings::OPENAI_CLIENT,
    error::ServerError,
    server::RustDocsServer,
};
use async_openai::{Client as OpenAIClient, config::OpenAIConfig};
use clap::Parser;
use std::env;
use rmcp::{
    transport::io::stdio,
    ServiceExt,
};
use futures::future::try_join_all;
use std::collections::HashMap;

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

    // Initialize OpenAI client (needed for query embedding)
    eprintln!("🤖 Initializing OpenAI client...");
    let openai_client = if let Ok(api_base) = env::var("OPENAI_API_BASE") {
        let config = OpenAIConfig::new().with_api_base(api_base);
        OpenAIClient::with_config(config)
    } else {
        OpenAIClient::new()
    };
    OPENAI_CLIENT
        .set(openai_client.clone())
        .expect("Failed to set OpenAI client");
    eprintln!("✅ OpenAI client initialized");

        // Load documents and embeddings from database IN PARALLEL
    eprintln!("🚀 Loading {} crates from database in parallel...", crate_names.len());
    let start_time = std::time::Instant::now();

    let load_tasks: Vec<_> = crate_names.iter().enumerate().map(|(i, crate_name)| {
        let db = &db;
        let crate_name = crate_name.clone();
        let total = crate_names.len();
        async move {
            eprintln!("  📦 [{}/{}] Loading crate: {}", i + 1, total, crate_name);
            let load_start = std::time::Instant::now();
            let documents = db.get_crate_documents(&crate_name).await?;
            let load_time = load_start.elapsed();
            eprintln!("  ✅ [{}/{}] Loaded {} documents from {} in {:.2}s",
                i + 1, total, documents.len(), crate_name, load_time.as_secs_f64());
            Ok::<_, ServerError>((crate_name, documents))
        }
    }).collect();

    let loaded_crates = try_join_all(load_tasks).await?;
    let total_load_time = start_time.elapsed();

    // Convert to the format expected by the server
    let mut all_documents = Vec::new();
    let mut all_embeddings = Vec::new();
    let mut crate_document_counts = HashMap::new();

    for (crate_name, crate_documents) in loaded_crates {
        if crate_documents.is_empty() {
            eprintln!("Warning: No documents found for crate '{}'", crate_name);
            continue;
        }

        let doc_count = crate_documents.len();
        crate_document_counts.insert(crate_name.clone(), doc_count);

        for (doc_path, content, embedding) in crate_documents {
            // Prefix the doc path with crate name to avoid conflicts
            let prefixed_path = format!("{}/{}", crate_name, doc_path);

            all_documents.push(Document {
                path: prefixed_path.clone(),
                content,
            });
            all_embeddings.push((prefixed_path, embedding));
        }
    }

    let total_docs = all_documents.len();
    let total_embeddings = all_embeddings.len();

    // Calculate total content size
    let total_content_size: usize = all_documents.iter().map(|doc| doc.content.len()).sum();
    let avg_doc_size = if total_docs > 0 { total_content_size / total_docs } else { 0 };

    eprintln!("\n📊 Loading Summary:");
    eprintln!("  ⏱️  Total loading time: {:.2}s", total_load_time.as_secs_f64());
    eprintln!("  📚 Total documents: {}", total_docs);
    eprintln!("  🧮 Total embeddings: {}", total_embeddings);
    eprintln!("  📄 Total content: {:.1} KB (avg: {:.1} KB/doc)",
        total_content_size as f64 / 1024.0, avg_doc_size as f64 / 1024.0);

    let startup_message = if crate_names.len() == 1 {
        format!(
            "Server for crate '{}' initialized. Loaded {} documents from database.",
            crate_names[0], total_docs
        )
    } else {
        let crate_summary: Vec<String> = crate_document_counts
            .iter()
            .map(|(name, count)| format!("{} ({})", name, count))
            .collect();
        format!(
            "Multi-crate server initialized. Loaded {} total documents from {} crates: {}",
            total_docs,
            crate_names.len(),
            crate_summary.join(", ")
        )
    };

    eprintln!("\n✅ {}", startup_message);

    // Create the service instance with combined data
    let combined_crate_name = if crate_names.len() == 1 {
        crate_names[0].clone()
    } else {
        format!("multi-crate[{}]", crate_names.join(","))
    };

    let service = RustDocsServer::new(
        combined_crate_name.clone(),
        all_documents,
        all_embeddings,
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
