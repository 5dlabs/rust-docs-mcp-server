use rustdocs_mcp_server::{
    database::Database,
    embeddings::{EMBEDDING_CLIENT, EmbeddingConfig, initialize_embedding_provider},
    error::ServerError,
};
use async_openai::{Client as OpenAIClient, config::OpenAIConfig};
use clap::Parser;
use rmcp::{
    ServerHandler, tool,
    transport::sse_server::{SseServer, SseServerConfig},
    service::{ServiceExt, RequestContext, RoleServer},
    model::{
        CallToolResult, Content, 
        ListResourcesResult, ListPromptsResult, 
        ListResourceTemplatesResult, ReadResourceResult, GetPromptResult,
        PaginatedRequestParam, ReadResourceRequestParam, GetPromptRequestParam,
        ProtocolVersion, ServerCapabilities, ServerInfo, Implementation,
        Resource, RawResource, ResourceContents, AnnotateAble,
    },
    Error as McpError,
};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use ndarray::Array1;
use std::{env, sync::Arc, net::SocketAddr};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(author, version, about = "Rust documentation MCP server with HTTP SSE transport", long_about = None)]
struct Cli {
    /// Port to listen on
    #[arg(short, long, default_value = "3000", env = "PORT")]
    port: u16,

    /// Host to bind to
    #[arg(long, default_value = "0.0.0.0", env = "HOST")]
    host: String,

    /// The crate names to serve documentation for (space-separated)
    #[arg(required = false)]
    crate_names: Vec<String>,

    /// Load all available crates from the database
    #[arg(short, long)]
    all: bool,

    /// Embedding provider to use (openai or voyage)
    #[arg(long, default_value = "openai", env = "EMBEDDING_PROVIDER")]
    embedding_provider: String,

    /// Embedding model to use
    #[arg(long, env = "EMBEDDING_MODEL")]
    embedding_model: Option<String>,
}

#[derive(Clone)]
struct McpHandler {
    database: Database,
    available_crates: Arc<Vec<String>>,
    startup_message: String,
}

impl McpHandler {
    fn new(database: Database, available_crates: Vec<String>, startup_message: String) -> Self {
        Self {
            database,
            available_crates: Arc::new(available_crates),
            startup_message,
        }
    }
    
    fn _create_resource_text(&self, uri: &str, name: &str) -> Resource {
        RawResource::new(uri, name.to_string()).no_annotation()
    }
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct QueryRustDocsArgs {
    /// The crate to search in (e.g., "axum", "tokio", "serde")
    crate_name: String,
    /// The specific question about the crate's API or usage.
    question: String,
}

// Implement ServerHandler trait with correct signatures
impl ServerHandler for McpHandler {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_logging()
            .build();

        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities,
            server_info: Implementation {
                name: "rustdocs-mcp-server-http".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            instructions: Some(self.startup_message.clone()),
        }
    }

    async fn list_resources(
        &self,
        _request: PaginatedRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![],
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        _request: ReadResourceRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        Err(McpError::invalid_request("No resources available".to_string(), None))
    }

    async fn list_prompts(
        &self,
        _request: PaginatedRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult {
            prompts: vec![],
            next_cursor: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        Err(McpError::invalid_params(
            format!("Prompt not found: {}", request.name),
            None,
        ))
    }

    async fn list_resource_templates(
        &self,
        _request: PaginatedRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![],
            next_cursor: None,
        })
    }
}

// Tool implementation
#[tool(tool_box)]
impl McpHandler {
    #[tool(
        description = "Query documentation for a specific Rust crate using semantic search and LLM summarization."
    )]
    async fn query_rust_docs(
        &self,
        #[tool(aggr)]
        args: QueryRustDocsArgs,
    ) -> Result<CallToolResult, McpError> {
        // Check if crate is available
        if !self.available_crates.contains(&args.crate_name) {
            return Err(McpError::invalid_params(
                format!(
                    "Crate '{}' not available. Available crates: {}",
                    args.crate_name,
                    self.available_crates.join(", ")
                ),
                None,
            ));
        }

        // Check if crate has embeddings in database
        if !self.database.has_embeddings(&args.crate_name).await.map_err(|e| {
            McpError::internal_error(e.to_string(), None)
        })? {
            return Err(McpError::invalid_params(
                format!(
                    "No embeddings found for crate '{}'. Please populate the database first.",
                    args.crate_name
                ),
                None,
            ));
        }

        // Generate embedding for the question
        let embedding_client = EMBEDDING_CLIENT.get()
            .ok_or_else(|| McpError::internal_error("Embedding client not initialized".to_string(), None))?;
        
        let (question_embeddings, _) = embedding_client.generate_embeddings(&[args.question.clone()]).await
            .map_err(|e| McpError::internal_error(format!("Failed to generate embedding: {}", e), None))?;
        
        let question_embedding = Array1::from_vec(question_embeddings.first()
            .ok_or_else(|| McpError::internal_error("No embedding generated".to_string(), None))?.clone());

        // Perform semantic search using the embedding
        match self.database.search_similar_docs(&args.crate_name, &question_embedding, 10).await {
            Ok(results) => {
                if results.is_empty() {
                    Ok(CallToolResult::success(vec![Content::text(format!(
                        "No relevant documentation found for '{}' in crate '{}'", 
                        args.question, args.crate_name
                    ))]))
                } else {
                    // Format search results - results are tuples (id, content, similarity)
                    let mut response = format!("From {} docs (via vector database search): ", args.crate_name);
                    
                    // Take top results and format them
                    let formatted_results: Vec<String> = results.into_iter()
                        .take(5) // Limit to top 5 results
                        .enumerate()
                        .map(|(i, (_, content, similarity))| {
                            format!("{}. {} (similarity: {:.3})", 
                                i + 1, 
                                content.trim(), 
                                similarity)
                        })
                        .collect();
                    
                    response.push_str(&formatted_results.join("\n\n"));
                    Ok(CallToolResult::success(vec![Content::text(response)]))
                }
            }
            Err(e) => Err(McpError::internal_error(format!("Database search error: {}", e), None))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), ServerError> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rustdocs_mcp_server_http=info,rmcp=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load .env file if present
    dotenvy::dotenv().ok();

    // Parse CLI arguments
    let cli = Cli::parse();

    info!("🚀 Starting Rust Docs MCP HTTP SSE Server on {}:{}", cli.host, cli.port);

    // Initialize database connection
    info!("🔌 Connecting to database...");
    let db = Database::new().await?;
    info!("✅ Database connected successfully");

    // Determine which crates to load
    let crate_names = if cli.all {
        info!("Loading all available crates from database...");
        let stats = db.get_crate_stats().await?;
        if stats.is_empty() {
            warn!("No crates found in database. Use 'populate_db' to add some first.");
            return Err(ServerError::Config("No crates in database".to_string()));
        }
        stats.into_iter().map(|stat| stat.name).collect()
    } else if cli.crate_names.is_empty() {
        // Default to all crates if none specified
        info!("No crates specified, loading all available crates...");
        let stats = db.get_crate_stats().await?;
        if stats.is_empty() {
            warn!("No crates found in database. Use 'populate_db' to add some first.");
            return Err(ServerError::Config("No crates in database".to_string()));
        }
        stats.into_iter().map(|stat| stat.name).collect()
    } else {
        cli.crate_names
    };

    info!("Target crates: {:?}", crate_names);

    // Check if all crates exist in database
    info!("🔍 Checking if crates exist in database...");
    let mut missing_crates = Vec::new();
    for crate_name in &crate_names {
        if !db.has_embeddings(crate_name).await? {
            missing_crates.push(crate_name.clone());
            warn!("❌ Missing: {}", crate_name);
        } else {
            info!("✅ Found: {}", crate_name);
        }
    }

    if !missing_crates.is_empty() {
        return Err(ServerError::Config(format!(
            "Missing crates in database: {:?}. Please populate them first using populate_db",
            missing_crates
        )));
    }

    // Initialize embedding provider (needed for query embedding)
    let provider_name = cli.embedding_provider.to_lowercase();
    info!("🤖 Initializing {} embedding provider...", provider_name);

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
    info!("✅ {} embedding provider initialized", provider_name);

    // Get crate statistics for startup message
    let stats = db.get_crate_stats().await?;
    let mut crate_stats = std::collections::HashMap::new();
    
    for crate_name in &crate_names {
        if let Some(stat) = stats.iter().find(|s| &s.name == crate_name) {
            crate_stats.insert(crate_name.clone(), stat.total_docs);
        }
    }

    let total_docs: i64 = crate_stats.values().map(|&v| v as i64).sum();

    // Create startup message
    let startup_message = if crate_names.len() == 1 {
        let doc_count = crate_stats.get(&crate_names[0]).unwrap_or(&0);
        format!(
            "HTTP SSE MCP server for crate '{}' initialized. {} documents available via database search.",
            crate_names[0], doc_count
        )
    } else {
        let crate_summary: Vec<String> = crate_stats
            .iter()
            .map(|(name, count)| format!("{} ({})", name, count))
            .collect();
        format!(
            "HTTP SSE MCP multi-crate server initialized. {} total documents available from {} crates: {}",
            total_docs,
            crate_names.len(),
            crate_summary.join(", ")
        )
    };

    info!("✅ {}", startup_message);

    // Create the MCP handler with database access
    let handler = McpHandler::new(db, crate_names, startup_message);

    // Create SSE server config
    let bind_addr: SocketAddr = format!("{}:{}", cli.host, cli.port).parse()
        .map_err(|e| ServerError::Config(format!("Invalid bind address: {}", e)))?;
    
    let config = SseServerConfig {
        bind: bind_addr,
        sse_path: "/sse".to_string(),
        post_path: "/message".to_string(),
        ct: CancellationToken::new(),
    };

    info!("🌐 Starting SSE server on {}", bind_addr);
    info!("📡 SSE endpoint: http://{}/sse", bind_addr);
    info!("📤 POST endpoint: http://{}/message", bind_addr);
    
    // Create and serve SSE server
    let mut sse_server = SseServer::serve_with_config(config).await
        .map_err(|e| ServerError::Internal(format!("Failed to start SSE server: {}", e)))?;

    info!("🔧 Server-Sent Events transport ready");
    info!("🎯 MCP server waiting for connections...");

    // Handle incoming transports
    while let Some(transport) = sse_server.next_transport().await {
        info!("🔗 New MCP connection established");
        let handler_clone = handler.clone();
        
        tokio::spawn(async move {
            match handler_clone.serve(transport).await {
                Ok(service) => {
                    if let Err(e) = service.waiting().await {
                        tracing::error!("MCP service error: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to start MCP service: {}", e);
                }
            }
        });
    }

    Ok(())
}