use crate::error::ServerError;
use ndarray::Array1;
use pgvector::Vector;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::env;

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn new() -> Result<Self, ServerError> {
        let database_url = env::var("MCPDOCS_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://jonathonfritz@localhost/rust_docs_vectors".to_string());

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .map_err(|e| ServerError::Database(format!("Failed to connect to database: {}", e)))?;

        Ok(Self { pool })
    }

    /// Insert or update a crate in the database
    pub async fn upsert_crate(&self, crate_name: &str, version: Option<&str>) -> Result<i32, ServerError> {
        let result = sqlx::query(
            r#"
            INSERT INTO crates (name, version)
            VALUES ($1, $2)
            ON CONFLICT (name)
            DO UPDATE SET
                version = COALESCE($2, crates.version),
                last_updated = CURRENT_TIMESTAMP
            RETURNING id
            "#
        )
        .bind(crate_name)
        .bind(version)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ServerError::Database(format!("Failed to upsert crate: {}", e)))?;

        let id: i32 = result.get("id");
        Ok(id)
    }

    /// Check if embeddings exist for a crate
    pub async fn has_embeddings(&self, crate_name: &str) -> Result<bool, ServerError> {
        let result = sqlx::query(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM doc_embeddings WHERE crate_name = $1
            ) as exists
            "#
        )
        .bind(crate_name)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ServerError::Database(format!("Failed to check embeddings: {}", e)))?;

        let exists: bool = result.get("exists");
        Ok(exists)
    }

    /// Insert a document embedding
    pub async fn insert_embedding(
        &self,
        crate_id: i32,
        crate_name: &str,
        doc_path: &str,
        content: &str,
        embedding: &Array1<f32>,
        token_count: i32,
    ) -> Result<(), ServerError> {
        let embedding_vec = Vector::from(embedding.to_vec());

        sqlx::query(
            r#"
            INSERT INTO doc_embeddings (crate_id, crate_name, doc_path, content, embedding, token_count)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (crate_name, doc_path)
            DO UPDATE SET
                content = $4,
                embedding = $5,
                token_count = $6,
                created_at = CURRENT_TIMESTAMP
            "#
        )
        .bind(crate_id)
        .bind(crate_name)
        .bind(doc_path)
        .bind(content)
        .bind(embedding_vec)
        .bind(token_count)
        .execute(&self.pool)
        .await
        .map_err(|e| ServerError::Database(format!("Failed to insert embedding: {}", e)))?;

        Ok(())
    }

    /// Batch insert multiple embeddings (more efficient)
    pub async fn insert_embeddings_batch(
        &self,
        crate_id: i32,
        crate_name: &str,
        embeddings: &[(String, String, Array1<f32>, i32)], // (path, content, embedding, token_count)
    ) -> Result<(), ServerError> {
        let mut tx = self.pool.begin().await
            .map_err(|e| ServerError::Database(format!("Failed to begin transaction: {}", e)))?;

        for (doc_path, content, embedding, token_count) in embeddings {
            let embedding_vec = Vector::from(embedding.to_vec());

            sqlx::query(
                r#"
                INSERT INTO doc_embeddings (crate_id, crate_name, doc_path, content, embedding, token_count)
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (crate_name, doc_path)
                DO UPDATE SET
                    content = $4,
                    embedding = $5,
                    token_count = $6,
                    created_at = CURRENT_TIMESTAMP
                "#
            )
            .bind(crate_id)
            .bind(crate_name)
            .bind(doc_path)
            .bind(content)
            .bind(embedding_vec)
            .bind(*token_count)
            .execute(&mut *tx)
            .await
            .map_err(|e| ServerError::Database(format!("Failed to insert embedding: {}", e)))?;
        }

        tx.commit().await
            .map_err(|e| ServerError::Database(format!("Failed to commit transaction: {}", e)))?;

        // Update crate statistics
        self.update_crate_stats(crate_id).await?;

        Ok(())
    }

    /// Update crate statistics
    async fn update_crate_stats(&self, crate_id: i32) -> Result<(), ServerError> {
        sqlx::query(
            r#"
            UPDATE crates
            SET total_docs = (
                SELECT COUNT(*) FROM doc_embeddings WHERE crate_id = $1
            ),
            total_tokens = (
                SELECT COALESCE(SUM(token_count), 0) FROM doc_embeddings WHERE crate_id = $1
            )
            WHERE id = $1
            "#
        )
        .bind(crate_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ServerError::Database(format!("Failed to update crate stats: {}", e)))?;

        Ok(())
    }

    /// Search for similar documents using vector similarity
    pub async fn search_similar_docs(
        &self,
        crate_name: &str,
        query_embedding: &Array1<f32>,
        limit: i32,
    ) -> Result<Vec<(String, String, f32)>, ServerError> {
        let embedding_vec = Vector::from(query_embedding.to_vec());

        let results = sqlx::query(
            r#"
            SELECT
                doc_path,
                content,
                1 - (embedding <=> $1) as similarity
            FROM doc_embeddings
            WHERE crate_name = $2
            ORDER BY embedding <=> $1
            LIMIT $3
            "#
        )
        .bind(embedding_vec)
        .bind(crate_name)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServerError::Database(format!("Failed to search documents: {}", e)))?;

        Ok(results
            .into_iter()
            .map(|row| {
                let doc_path: String = row.get("doc_path");
                let content: String = row.get("content");
                let similarity: f64 = row.get("similarity");
                let similarity = similarity as f32; // Convert to f32 for compatibility
                (doc_path, content, similarity)
            })
            .collect())
    }

    /// Get all documents for a crate (for loading into memory if needed)
    pub async fn get_crate_documents(
        &self,
        crate_name: &str,
    ) -> Result<Vec<(String, String, Array1<f32>)>, ServerError> {
        eprintln!("    🔍 Querying database for crate: {}", crate_name);
        let query_start = std::time::Instant::now();

        let results = sqlx::query(
            r#"
            SELECT doc_path, content, embedding
            FROM doc_embeddings
            WHERE crate_name = $1
            ORDER BY doc_path
            "#
        )
        .bind(crate_name)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServerError::Database(format!("Failed to get crate documents: {}", e)))?;

        let query_time = query_start.elapsed();
        eprintln!("    📊 Found {} documents for {} in {:.3}s", results.len(), crate_name, query_time.as_secs_f64());

                let mut documents = Vec::new();
        for (i, row) in results.iter().enumerate() {
            let doc_path: String = row.get("doc_path");
            let content: String = row.get("content");
            let embedding_vec: Vector = row.get("embedding");
            let embedding_array = Array1::from_vec(embedding_vec.to_vec());

            if i < 3 || (i + 1) % 5 == 0 {
                eprintln!("    📄 [{}/{}] Processed: {} ({} chars, {} dims)",
                    i + 1, results.len(), doc_path, content.len(), embedding_array.len());
            }

            documents.push((doc_path, content, embedding_array));
        }

        Ok(documents)
    }

    /// Delete all embeddings for a crate
    pub async fn delete_crate_embeddings(&self, crate_name: &str) -> Result<(), ServerError> {
        sqlx::query(
            r#"
            DELETE FROM doc_embeddings WHERE crate_name = $1
            "#
        )
        .bind(crate_name)
        .execute(&self.pool)
        .await
        .map_err(|e| ServerError::Database(format!("Failed to delete embeddings: {}", e)))?;

        Ok(())
    }

    /// Get crate statistics
    pub async fn get_crate_stats(&self) -> Result<Vec<CrateStats>, ServerError> {
        let results = sqlx::query(
            r#"
            SELECT
                name,
                version,
                last_updated,
                total_docs,
                total_tokens
            FROM crates
            ORDER BY name
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ServerError::Database(format!("Failed to get crate stats: {}", e)))?;

        Ok(results
            .into_iter()
            .map(|row| {
                let name: String = row.get("name");
                let version: Option<String> = row.get("version");
                let last_updated: chrono::NaiveDateTime = row.get("last_updated");
                let total_docs: Option<i32> = row.get("total_docs");
                let total_tokens: Option<i32> = row.get("total_tokens");

                CrateStats {
                    name,
                    version,
                    last_updated,
                    total_docs: total_docs.unwrap_or(0),
                    total_tokens: total_tokens.unwrap_or(0),
                }
            })
            .collect())
    }
}

#[derive(Debug)]
pub struct CrateStats {
    pub name: String,
    pub version: Option<String>,
    pub last_updated: chrono::NaiveDateTime,
    pub total_docs: i32,
    pub total_tokens: i32,
}