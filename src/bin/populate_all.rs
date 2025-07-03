use rustdocs_mcp_server::{
    database::Database,
    doc_loader,
    embeddings::{generate_embeddings, EMBEDDING_CLIENT, EmbeddingConfig, initialize_embedding_provider},
    error::ServerError,
};
use async_openai::{Client as OpenAIClient, config::OpenAIConfig};
use serde::{Deserialize, Serialize};
use std::{env, fs};
use futures::future::try_join_all;
use tiktoken_rs;

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
}

#[tokio::main]
async fn main() -> Result<(), ServerError> {
    dotenvy::dotenv().ok();

    // Read proxy-config.json
    println!("📋 Reading proxy-config.json...");
    let config_content = fs::read_to_string("proxy-config.json")
        .map_err(|e| ServerError::Config(format!("Failed to read proxy-config.json: {}", e)))?;

    let config: ProxyConfig = serde_json::from_str(&config_content)
        .map_err(|e| ServerError::Config(format!("Failed to parse proxy-config.json: {}", e)))?;

    // Filter enabled crates
    let enabled_crates: Vec<_> = config.crates.into_iter()
        .filter(|c| c.enabled)
        .collect();

    println!("📦 Found {} enabled crates to populate", enabled_crates.len());
    for crate_config in &enabled_crates {
        println!("  - {} {:?}", crate_config.name, crate_config.features);
    }

    // Initialize database
    let db = Database::new().await?;

    // Check which crates already exist
    let mut crates_to_populate = Vec::new();
    for crate_config in &enabled_crates {
        if db.has_embeddings(&crate_config.name).await? {
            println!("✅ {} already has embeddings", crate_config.name);
        } else {
            println!("❌ {} needs to be populated", crate_config.name);
            crates_to_populate.push(crate_config);
        }
    }

    if crates_to_populate.is_empty() {
        println!("✅ All crates already have embeddings!");
        return Ok(());
    }

    // Initialize embedding provider (default to OpenAI for populate script)
    let provider_type = env::var("EMBEDDING_PROVIDER").unwrap_or_else(|_| "openai".to_string());
    let embedding_config = match provider_type.to_lowercase().as_str() {
        "openai" => {
            let model = env::var("EMBEDDING_MODEL").unwrap_or_else(|_| "text-embedding-3-large".to_string());
            let openai_client = if let Ok(api_base) = env::var("OPENAI_API_BASE") {
                let config = OpenAIConfig::new().with_api_base(api_base);
                OpenAIClient::with_config(config)
            } else {
                OpenAIClient::new()
            };
            EmbeddingConfig::OpenAI { client: openai_client, model }
        },
        "voyage" => {
            let api_key = env::var("VOYAGE_API_KEY")
                .map_err(|_| ServerError::MissingEnvVar("VOYAGE_API_KEY".to_string()))?;
            let model = env::var("EMBEDDING_MODEL").unwrap_or_else(|_| "voyage-3.5".to_string());
            EmbeddingConfig::VoyageAI { api_key, model }
        },
        _ => {
            return Err(ServerError::Config(format!(
                "Unsupported embedding provider: {}. Use 'openai' or 'voyage'",
                provider_type
            )));
        }
    };

    let provider = initialize_embedding_provider(embedding_config);
    if EMBEDDING_CLIENT.set(provider).is_err() {
        return Err(ServerError::Internal("Failed to set embedding provider".to_string()));
    }

    let embedding_model = env::var("EMBEDDING_MODEL")
        .unwrap_or_else(|_| "text-embedding-3-small".to_string());

    println!("\n🚀 Starting parallel population of {} crates...", crates_to_populate.len());
    let start_time = std::time::Instant::now();

    // Create tasks for parallel processing
    let tasks: Vec<_> = crates_to_populate.into_iter().enumerate().map(|(i, crate_config)| {
        let db = &db;
        // Provider is now globally accessible, no cloning needed
        let crate_name = crate_config.name.clone();
        let features = crate_config.features.clone();
        let total = enabled_crates.len();

        async move {
            println!("\n📥 [{}/{}] Loading documentation for: {}", i + 1, total, crate_name);
            let doc_start = std::time::Instant::now();

            let load_result = doc_loader::load_documents_from_docs_rs(
                &crate_name,
                "*",
                features.as_ref(),
                Some(50)  // Use smaller page limit for batch processing
            ).await?;
            let documents = load_result.documents;
            let crate_version = load_result.version;

            let doc_time = doc_start.elapsed();
            println!("✅ [{}/{}] Loaded {} documents for {} in {:.2}s",
                i + 1, total, documents.len(), crate_name, doc_time.as_secs_f64());

            if let Some(ref version) = crate_version {
                println!("📦 [{}/{}] Detected version for {}: {}", i + 1, total, crate_name, version);
            }

            if documents.is_empty() {
                println!("⚠️  No documents found for {}", crate_name);
                return Ok::<_, ServerError>((crate_name, 0, 0.0));
            }

            // Generate embeddings
            println!("🧠 [{}/{}] Generating embeddings for {}...", i + 1, total, crate_name);
            let embed_start = std::time::Instant::now();
            let (embeddings, total_tokens) = generate_embeddings(&documents).await?;
            let embed_time = embed_start.elapsed();

            let cost_per_million = 0.02;
            let estimated_cost = (total_tokens as f64 / 1_000_000.0) * cost_per_million;
            println!("✅ [{}/{}] Generated {} embeddings for {} in {:.2}s (${:.6})",
                i + 1, total, embeddings.len(), crate_name, embed_time.as_secs_f64(), estimated_cost);

            // Store in database
            let crate_id = db.upsert_crate(&crate_name, crate_version.as_deref()).await?;

            // Initialize tokenizer for accurate token counting
            let bpe = tiktoken_rs::cl100k_base()
                .map_err(|e| ServerError::Tiktoken(e.to_string()))?;

            let mut batch_data = Vec::new();
            for (path, content, embedding) in embeddings.iter() {
                // Calculate actual token count for this chunk
                let token_count = bpe.encode_with_special_tokens(content).len() as i32;
                batch_data.push((
                    path.clone(),
                    content.clone(),
                    embedding.clone(),
                    token_count,
                ));
            }

            db.insert_embeddings_batch(crate_id, &crate_name, &batch_data).await?;

            // Add delay between crates to be respectful to docs.rs
            if i < total - 1 {
                println!("⏱️  Waiting 2 seconds before next crate...");
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }

            Ok((crate_name, embeddings.len(), estimated_cost))
        }
    }).collect();

    // Execute all tasks in parallel
    let results = try_join_all(tasks).await?;
    let total_time = start_time.elapsed();

    // Summary
    println!("\n🎉 Population complete! Total time: {:.2}s", total_time.as_secs_f64());
    println!("📊 Summary:");

    let mut total_embeddings = 0;
    let mut total_cost = 0.0;

    for (crate_name, embedding_count, cost) in results {
        println!("  ✅ {}: {} embeddings (${:.6})", crate_name, embedding_count, cost);
        total_embeddings += embedding_count;
        total_cost += cost;
    }

    println!("\n📈 Total: {} embeddings across {} crates", total_embeddings, enabled_crates.len());
    println!("💰 Total estimated cost: ${:.6}", total_cost);

    Ok(())
}