use rustdocs_mcp_server::{
    database::Database,
    embeddings::{EmbeddingConfig, initialize_embedding_provider, EMBEDDING_CLIENT},
    error::ServerError,
};
use async_openai::{Client as OpenAIClient, config::OpenAIConfig};
use ndarray::Array1;
use std::env;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), ServerError> {
    // Load .env file if present
    dotenvy::dotenv().ok();
    
    println!("🔬 Comprehensive Vector Search Analysis\n");
    
    // Initialize database connection
    println!("🔌 Connecting to database...");
    let db = Database::new().await?;
    println!("✅ Database connected successfully\n");
    
    // Initialize embedding provider
    println!("🤖 Initializing OpenAI embedding provider...");
    let openai_client = if let Ok(api_base) = env::var("OPENAI_API_BASE") {
        let config = OpenAIConfig::new().with_api_base(api_base);
        OpenAIClient::with_config(config)
    } else {
        OpenAIClient::new()
    };
    
    let embedding_config = EmbeddingConfig::OpenAI {
        client: openai_client,
        model: "text-embedding-ada-002".to_string(),
    };
    
    let provider = initialize_embedding_provider(embedding_config);
    if EMBEDDING_CLIENT.set(provider).is_err() {
        return Err(ServerError::Internal("Failed to set embedding provider".to_string()));
    }
    println!("✅ OpenAI embedding provider initialized\n");
    
    // Get statistics for all crates
    println!("📊 Crate Statistics:");
    let stats = db.get_crate_stats().await?;
    let mut crate_stats: HashMap<String, (i32, i32)> = HashMap::new();
    
    println!("{:<20} {:<10} {:<10}", "Crate", "Docs", "Tokens");
    println!("{:-<40}", "");
    for stat in stats {
        println!("{:<20} {:<10} {:<10}", stat.name, stat.total_docs, stat.total_tokens);
        crate_stats.insert(stat.name.clone(), (stat.total_docs, stat.total_tokens));
    }
    
    // Test crates
    let test_crates = vec!["axum", "tokio", "serde"];
    let test_queries = vec![
        ("axum", vec!["router", "handler", "middleware", "extract"]),
        ("tokio", vec!["spawn", "runtime", "async", "task"]),
        ("serde", vec!["serialize", "deserialize", "derive", "json"]),
    ];
    
    println!("\n🧪 Testing Vector Search for Multiple Crates:\n");
    
    for (crate_name, queries) in test_queries {
        if !crate_stats.contains_key(crate_name) {
            println!("⚠️  Skipping {} - not found in database", crate_name);
            continue;
        }
        
        println!("📦 Testing {}", crate_name);
        let (total_docs, _) = crate_stats.get(crate_name).unwrap();
        println!("   Total documents: {}", total_docs);
        
        // Load documents to check content
        let docs = db.get_crate_documents(crate_name).await?;
        
        // Check document paths
        let mut path_types: HashMap<String, usize> = HashMap::new();
        for (path, _, _) in &docs {
            let path_type = if path.contains("struct") { "struct" }
                else if path.contains("trait") { "trait" }
                else if path.contains("fn") { "function" }
                else if path.contains("enum") { "enum" }
                else if path.contains("mod") { "module" }
                else { "other" };
            *path_types.entry(path_type.to_string()).or_insert(0) += 1;
        }
        
        println!("   Document types:");
        for (path_type, count) in &path_types {
            println!("     - {}: {}", path_type, count);
        }
        
        // Check content statistics
        let total_content_size: usize = docs.iter().map(|(_, content, _)| content.len()).sum();
        let avg_content_size = if !docs.is_empty() { total_content_size / docs.len() } else { 0 };
        println!("   Average content size: {} chars", avg_content_size);
        
        // Check embedding statistics
        if let Some((_, _, embedding)) = docs.first() {
            let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            println!("   Embedding dimensions: {}", embedding.len());
            println!("   Sample embedding L2 norm: {:.6}", norm);
        }
        
        println!("\n   Testing queries:");
        let embedding_provider = EMBEDDING_CLIENT.get().unwrap();
        
        for query in queries {
            print!("   - Query '{}': ", query);
            
            // Generate embedding for query
            let (embeddings, _) = embedding_provider
                .generate_embeddings(&[query.to_string()])
                .await?;
            let query_embedding = embeddings.into_iter().next().unwrap();
            let query_vector = Array1::from(query_embedding);
            
            // Search
            let results = db.search_similar_docs(crate_name, &query_vector, 3).await?;
            
            if results.is_empty() {
                println!("❌ No results");
            } else {
                println!("✅ {} results", results.len());
                for (i, (path, _, score)) in results.iter().enumerate() {
                    println!("       {}. {} (similarity: {:.4})", i + 1, path, score);
                }
            }
        }
        
        println!();
    }
    
    // Specific axum debugging
    println!("\n🔍 Deep Dive: Axum Router Analysis");
    
    let axum_docs = db.get_crate_documents("axum").await?;
    
    // Find all router-related documents
    let router_docs: Vec<_> = axum_docs.iter()
        .filter(|(path, content, _)| {
            let path_lower = path.to_lowercase();
            let content_lower = content.to_lowercase();
            path_lower.contains("router") || content_lower.contains("router")
        })
        .collect();
    
    println!("   Found {} router-related documents out of {} total", router_docs.len(), axum_docs.len());
    
    if !router_docs.is_empty() {
        println!("\n   Sample router documents:");
        for (path, content, _) in router_docs.iter().take(5) {
            println!("   - {}", path);
            // Find the first occurrence of "router" in content
            if let Some(pos) = content.to_lowercase().find("router") {
                let start = pos.saturating_sub(50);
                let end = (pos + 50).min(content.len());
                let snippet = &content[start..end];
                println!("     Context: ...{}...", snippet);
            }
        }
        
        // Test with actual router document embedding
        println!("\n   Testing with actual router document embedding:");
        if let Some((path, _, embedding)) = router_docs.first() {
            println!("   Using document: {}", path);
            
            let results = db.search_similar_docs("axum", embedding, 5).await?;
            
            println!("   Similar documents:");
            for (i, (result_path, _, score)) in results.iter().enumerate() {
                println!("   {}. {} (similarity: {:.4})", i + 1, result_path, score);
            }
        }
    }
    
    // Compare embeddings between working and non-working searches
    println!("\n📐 Embedding Comparison:");
    
    // Get a sample embedding from each crate
    let mut sample_embeddings: HashMap<String, Array1<f32>> = HashMap::new();
    
    for crate_name in &["axum", "tokio"] {
        if let Ok(docs) = db.get_crate_documents(crate_name).await {
            if let Some((_, _, embedding)) = docs.first() {
                sample_embeddings.insert(crate_name.to_string(), embedding.clone());
            }
        }
    }
    
    // Calculate cosine similarity between embeddings
    if sample_embeddings.len() == 2 {
        let axum_emb = sample_embeddings.get("axum").unwrap();
        let tokio_emb = sample_embeddings.get("tokio").unwrap();
        
        let dot_product: f32 = axum_emb.iter().zip(tokio_emb.iter()).map(|(a, b)| a * b).sum();
        let axum_norm: f32 = axum_emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        let tokio_norm: f32 = tokio_emb.iter().map(|x| x * x).sum::<f32>().sqrt();
        let cosine_sim = dot_product / (axum_norm * tokio_norm);
        
        println!("   Sample embedding cosine similarity (axum vs tokio): {:.4}", cosine_sim);
        println!("   This should be low (~0.1-0.3) as they are from different domains");
    }
    
    println!("\n✅ Analysis complete!");
    
    Ok(())
}