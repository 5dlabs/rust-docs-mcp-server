use rustdocs_mcp_server::{
    database::Database,
    embeddings::{EmbeddingConfig, initialize_embedding_provider, EMBEDDING_CLIENT},
    error::ServerError,
};
use async_openai::{Client as OpenAIClient, config::OpenAIConfig};
use ndarray::Array1;
use std::env;

#[tokio::main]
async fn main() -> Result<(), ServerError> {
    // Load .env file if present
    dotenvy::dotenv().ok();
    
    println!("🔍 Debugging Axum Vector Search Issues\n");
    
    // Initialize database connection
    println!("🔌 Connecting to database...");
    let db = Database::new().await?;
    println!("✅ Database connected successfully\n");
    
    // Initialize embedding provider (needed for query embedding)
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
    
    // 1. Check if axum documents actually have content in the database
    println!("📊 1. Checking axum documents in database...");
    let axum_docs = db.get_crate_documents("axum").await?;
    println!("   Found {} axum documents", axum_docs.len());
    
    if axum_docs.is_empty() {
        println!("   ❌ No axum documents found in database!");
        return Ok(());
    }
    
    // 2. Look at a few sample axum documents to see their structure
    println!("\n📄 2. Sample axum documents:");
    for (i, (path, content, embedding)) in axum_docs.iter().take(5).enumerate() {
        println!("\n   Document {}: {}", i + 1, path);
        println!("   Content length: {} chars", content.len());
        println!("   Embedding dimensions: {}", embedding.len());
        println!("   Content preview (first 200 chars):");
        println!("   {}", &content.chars().take(200).collect::<String>());
        
        // Check for router-related content
        if content.to_lowercase().contains("router") || content.to_lowercase().contains("route") {
            println!("   ✅ Contains router/route keywords!");
        }
    }
    
    // Check specifically for router-related documents
    println!("\n🔍 Looking for router-related documents in axum:");
    let router_docs: Vec<_> = axum_docs.iter()
        .filter(|(path, content, _)| {
            path.to_lowercase().contains("router") || 
            path.to_lowercase().contains("route") ||
            content.to_lowercase().contains("router") ||
            content.to_lowercase().contains("route")
        })
        .collect();
    
    println!("   Found {} router-related documents", router_docs.len());
    for (path, _, _) in router_docs.iter().take(10) {
        println!("   - {}", path);
    }
    
    // 3. Test a direct similarity search for "router" or "route" keywords in axum docs
    println!("\n🧪 3. Testing vector search for 'router' in axum:");
    
    // Generate embedding for the query
    let query = "router";
    let embedding_provider = EMBEDDING_CLIENT.get().unwrap();
    let (embeddings, _) = embedding_provider
        .generate_embeddings(&[query.to_string()])
        .await?;
    let query_embedding = embeddings.into_iter().next().unwrap();
    let query_vector = Array1::from(query_embedding);
    
    // Search for similar documents
    let search_results = db.search_similar_docs("axum", &query_vector, 5).await?;
    
    if search_results.is_empty() {
        println!("   ❌ No results found for 'router' query!");
    } else {
        println!("   ✅ Found {} results:", search_results.len());
        for (i, (path, content, score)) in search_results.iter().enumerate() {
            println!("\n   Result {}: {} (similarity: {:.4})", i + 1, path, score);
            println!("   Content preview: {}", &content.chars().take(150).collect::<String>());
        }
    }
    
    // 4. Compare the content structure between working crates (tokio) and non-working (axum)
    println!("\n🔄 4. Comparing with tokio (working crate):");
    
    // Get tokio documents
    let tokio_docs = db.get_crate_documents("tokio").await?;
    println!("   Found {} tokio documents", tokio_docs.len());
    
    if !tokio_docs.is_empty() {
        println!("\n   Sample tokio document:");
        let (path, content, embedding) = &tokio_docs[0];
        println!("   Path: {}", path);
        println!("   Content length: {} chars", content.len());
        println!("   Embedding dimensions: {}", embedding.len());
        println!("   Content preview: {}", &content.chars().take(200).collect::<String>());
        
        // Test vector search on tokio
        println!("\n   Testing vector search for 'spawn' in tokio:");
        let query = "spawn";
        let (embeddings, _) = embedding_provider
            .generate_embeddings(&[query.to_string()])
            .await?;
        let query_embedding = embeddings.into_iter().next().unwrap();
        let query_vector = Array1::from(query_embedding);
        
        let search_results = db.search_similar_docs("tokio", &query_vector, 3).await?;
        
        if search_results.is_empty() {
            println!("   ❌ No results found for 'spawn' query in tokio!");
        } else {
            println!("   ✅ Found {} results in tokio:", search_results.len());
            for (i, (path, _, score)) in search_results.iter().enumerate() {
                println!("   Result {}: {} (similarity: {:.4})", i + 1, path, score);
            }
        }
    }
    
    // Additional diagnostics: Check embedding statistics
    println!("\n📈 5. Embedding statistics:");
    
    // Check if embeddings are normalized
    if let Some((path, _, embedding)) = axum_docs.first() {
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        println!("   Sample axum embedding L2 norm: {:.6}", norm);
        println!("   Is normalized (norm ≈ 1.0)?: {}", (norm - 1.0).abs() < 0.01);
    }
    
    if let Some((path, _, embedding)) = tokio_docs.first() {
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        println!("   Sample tokio embedding L2 norm: {:.6}", norm);
        println!("   Is normalized (norm ≈ 1.0)?: {}", (norm - 1.0).abs() < 0.01);
    }
    
    // Check embedding dimensions
    if let Some((_, _, embedding)) = axum_docs.first() {
        println!("\n   Axum embedding dimensions: {}", embedding.len());
    }
    if let Some((_, _, embedding)) = tokio_docs.first() {
        println!("   Tokio embedding dimensions: {}", embedding.len());
    }
    
    println!("\n✅ Diagnostic complete!");
    
    Ok(())
}