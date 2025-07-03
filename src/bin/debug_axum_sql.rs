use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file if present
    dotenvy::dotenv().ok();
    
    println!("🔍 Direct SQL Diagnostics for Axum Documentation\n");
    
    // Connect to database
    let database_url = env::var("MCPDOCS_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://jonathonfritz@localhost/rust_docs_vectors".to_string());
    
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    
    println!("✅ Connected to database\n");
    
    // 1. Check crate statistics
    println!("📊 1. Crate Statistics:");
    let stats = sqlx::query(
        r#"
        SELECT name, version, total_docs, total_tokens, last_updated
        FROM crates
        WHERE name IN ('axum', 'tokio')
        ORDER BY name
        "#
    )
    .fetch_all(&pool)
    .await?;
    
    for row in stats {
        let name: String = row.get("name");
        let version: Option<String> = row.get("version");
        let total_docs: Option<i32> = row.get("total_docs");
        let total_tokens: Option<i32> = row.get("total_tokens");
        let last_updated: chrono::NaiveDateTime = row.get("last_updated");
        
        println!("   {} (v{}): {} docs, {} tokens, updated: {}", 
            name, 
            version.unwrap_or("unknown".to_string()),
            total_docs.unwrap_or(0),
            total_tokens.unwrap_or(0),
            last_updated.format("%Y-%m-%d %H:%M")
        );
    }
    
    // 2. Sample axum documents
    println!("\n📄 2. Sample Axum Documents:");
    let axum_samples = sqlx::query(
        r#"
        SELECT doc_path, 
               LENGTH(content) as content_length,
               array_length(embedding, 1) as embedding_dims,
               LEFT(content, 200) as content_preview
        FROM doc_embeddings
        WHERE crate_name = 'axum'
        LIMIT 10
        "#
    )
    .fetch_all(&pool)
    .await?;
    
    for (i, row) in axum_samples.iter().enumerate() {
        let doc_path: String = row.get("doc_path");
        let content_length: Option<i32> = row.get("content_length");
        let embedding_dims: Option<i32> = row.get("embedding_dims");
        let content_preview: String = row.get("content_preview");
        
        println!("\n   Document {}: {}", i + 1, doc_path);
        println!("   Content length: {} chars", content_length.unwrap_or(0));
        println!("   Embedding dimensions: {}", embedding_dims.unwrap_or(0));
        println!("   Preview: {}", content_preview);
    }
    
    // 3. Search for router-related documents
    println!("\n🔍 3. Router-related Documents in Axum:");
    let router_docs = sqlx::query(
        r#"
        SELECT doc_path, 
               LENGTH(content) as content_length
        FROM doc_embeddings
        WHERE crate_name = 'axum'
          AND (LOWER(doc_path) LIKE '%router%' 
               OR LOWER(doc_path) LIKE '%route%'
               OR LOWER(content) LIKE '%router%'
               OR LOWER(content) LIKE '%route%')
        LIMIT 20
        "#
    )
    .fetch_all(&pool)
    .await?;
    
    println!("   Found {} router-related documents:", router_docs.len());
    for row in router_docs {
        let doc_path: String = row.get("doc_path");
        let content_length: Option<i32> = row.get("content_length");
        println!("   - {} ({} chars)", doc_path, content_length.unwrap_or(0));
    }
    
    // 4. Check embedding validity
    println!("\n🧮 4. Embedding Validity Check:");
    
    // Check if embeddings are normalized (L2 norm should be ~1.0 for OpenAI embeddings)
    let norm_check = sqlx::query(
        r#"
        SELECT crate_name,
               doc_path,
               sqrt(sum(val * val)) as l2_norm
        FROM (
            SELECT crate_name, 
                   doc_path,
                   unnest(embedding) as val
            FROM doc_embeddings
            WHERE crate_name IN ('axum', 'tokio')
            LIMIT 2
        ) t
        GROUP BY crate_name, doc_path
        "#
    )
    .fetch_all(&pool)
    .await?;
    
    for row in norm_check {
        let crate_name: String = row.get("crate_name");
        let doc_path: String = row.get("doc_path");
        let l2_norm: Option<f64> = row.get("l2_norm");
        
        println!("   {} - {}: L2 norm = {:.6}", 
            crate_name, doc_path, l2_norm.unwrap_or(0.0));
    }
    
    // 5. Test direct vector similarity calculation
    println!("\n🧪 5. Direct Vector Similarity Test:");
    
    // Get a sample embedding from axum
    let sample_result = sqlx::query(
        r#"
        SELECT embedding
        FROM doc_embeddings
        WHERE crate_name = 'axum'
          AND LOWER(content) LIKE '%router%'
        LIMIT 1
        "#
    )
    .fetch_optional(&pool)
    .await?;
    
    if let Some(sample_row) = sample_result {
        println!("   Found a router-related document to use as query");
        
        // Test similarity search using this embedding
        let test_search = sqlx::query(
            r#"
            SELECT doc_path,
                   1 - (embedding <=> $1) as similarity
            FROM doc_embeddings
            WHERE crate_name = 'axum'
            ORDER BY embedding <=> $1
            LIMIT 5
            "#
        )
        .bind::<pgvector::Vector>(sample_row.get("embedding"))
        .fetch_all(&pool)
        .await?;
        
        println!("   Top 5 similar documents:");
        for row in test_search {
            let doc_path: String = row.get("doc_path");
            let similarity: f64 = row.get("similarity");
            println!("   - {} (similarity: {:.4})", doc_path, similarity);
        }
    } else {
        println!("   ❌ No router-related documents found to test with");
    }
    
    // 6. Check for NULL or empty embeddings
    println!("\n❓ 6. Checking for NULL or empty embeddings:");
    let null_check = sqlx::query(
        r#"
        SELECT crate_name,
               COUNT(*) FILTER (WHERE embedding IS NULL) as null_embeddings,
               COUNT(*) FILTER (WHERE array_length(embedding, 1) = 0) as empty_embeddings,
               COUNT(*) FILTER (WHERE array_length(embedding, 1) != 1536) as wrong_dim_embeddings,
               COUNT(*) as total
        FROM doc_embeddings
        WHERE crate_name IN ('axum', 'tokio')
        GROUP BY crate_name
        "#
    )
    .fetch_all(&pool)
    .await?;
    
    for row in null_check {
        let crate_name: String = row.get("crate_name");
        let null_embeddings: Option<i64> = row.get("null_embeddings");
        let empty_embeddings: Option<i64> = row.get("empty_embeddings");
        let wrong_dim_embeddings: Option<i64> = row.get("wrong_dim_embeddings");
        let total: Option<i64> = row.get("total");
        
        println!("   {}: {} total, {} NULL, {} empty, {} wrong dimensions", 
            crate_name, 
            total.unwrap_or(0),
            null_embeddings.unwrap_or(0),
            empty_embeddings.unwrap_or(0),
            wrong_dim_embeddings.unwrap_or(0)
        );
    }
    
    println!("\n✅ SQL diagnostics complete!");
    
    Ok(())
}