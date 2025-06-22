use crate::{doc_loader::Document, error::ServerError};
use async_openai::{
    config::OpenAIConfig, error::ApiError as OpenAIAPIErr, types::CreateEmbeddingRequestArgs,
    Client as OpenAIClient,
};
use ndarray::{Array1, ArrayView1};
use std::sync::OnceLock;
use std::sync::Arc;
use tiktoken_rs::cl100k_base;
use futures::stream::{self, StreamExt};

// Static OnceLock for the OpenAI client
pub static OPENAI_CLIENT: OnceLock<OpenAIClient<OpenAIConfig>> = OnceLock::new();


use bincode::{Encode, Decode};
use serde::{Serialize, Deserialize};

// Define a struct containing path, content, and embedding for caching
#[derive(Serialize, Deserialize, Debug, Encode, Decode)]
pub struct CachedDocumentEmbedding {
    pub path: String,
    pub content: String, // Add the extracted document content
    pub vector: Vec<f32>,
}


/// Calculates the cosine similarity between two vectors.
pub fn cosine_similarity(v1: ArrayView1<f32>, v2: ArrayView1<f32>) -> f32 {
    let dot_product = v1.dot(&v2);
    let norm_v1 = v1.dot(&v1).sqrt();
    let norm_v2 = v2.dot(&v2).sqrt();

    if norm_v1 == 0.0 || norm_v2 == 0.0 {
        0.0
    } else {
        dot_product / (norm_v1 * norm_v2)
    }
}

/// Splits content into chunks that fit within the token limit
fn _chunk_content(content: &str, bpe: &tiktoken_rs::CoreBPE, token_limit: usize) -> Vec<String> {
    let tokens = bpe.encode_with_special_tokens(content);

    if tokens.len() <= token_limit {
        return vec![content.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current_chunk_tokens = Vec::new();

    // Split by sentences first (simple approach - split on ". ")
    let sentences: Vec<&str> = content.split(". ").collect();
    let mut current_chunk_sentences = Vec::new();

    for sentence in sentences {
        let sentence_with_period = if sentence.ends_with('.') {
            sentence.to_string()
        } else {
            format!("{}.", sentence)
        };

        let sentence_tokens = bpe.encode_with_special_tokens(&sentence_with_period);

        // If adding this sentence would exceed the limit, save current chunk
        if !current_chunk_tokens.is_empty() && current_chunk_tokens.len() + sentence_tokens.len() > token_limit {
            let chunk_text = current_chunk_sentences.join(" ");
            chunks.push(chunk_text);
            current_chunk_sentences.clear();
            current_chunk_tokens.clear();
        }

        // If a single sentence exceeds the limit, we need to split it further
        if sentence_tokens.len() > token_limit {
            // For now, skip sentences that are too long
            eprintln!("Warning: Single sentence exceeds token limit, splitting by tokens");

            // Split by tokens directly
            let mut start = 0;
            while start < tokens.len() {
                let end = std::cmp::min(start + token_limit, tokens.len());
                let chunk_tokens = &tokens[start..end];
                if let Ok(chunk_text) = bpe.decode(chunk_tokens.to_vec()) {
                    chunks.push(chunk_text);
                }
                start = end;
            }
            continue;
        }

        current_chunk_sentences.push(sentence_with_period);
        current_chunk_tokens.extend(sentence_tokens);
    }

    // Don't forget the last chunk
    if !current_chunk_sentences.is_empty() {
        let chunk_text = current_chunk_sentences.join(" ");
        chunks.push(chunk_text);
    }

    chunks
}



/// Generates embeddings for a list of documents using the OpenAI API with chunking support.
#[allow(dead_code)]
pub async fn generate_embeddings(
    client: &OpenAIClient<OpenAIConfig>,
    documents: &[Document],
    model: &str,
) -> Result<(Vec<(String, String, Array1<f32>)>, usize), ServerError> { // Return tuple: (path, content, embedding), total_tokens
    eprintln!("Generating embeddings for {} documents using model '{}'...", documents.len(), model);

    // Get the tokenizer for the model and wrap in Arc
    let bpe = Arc::new(cl100k_base().map_err(|e| ServerError::Tiktoken(e.to_string()))?);

    const CONCURRENCY_LIMIT: usize = 8; // Number of concurrent requests
    const TOKEN_LIMIT: usize = 8000; // Keep a buffer below the 8192 limit
    const CHUNK_OVERLAP: usize = 200; // Token overlap between chunks for context

    // First, prepare all chunks with their metadata
    let mut all_chunks = Vec::new();
    for (doc_index, doc) in documents.iter().enumerate() {
        let token_count = bpe.encode_with_special_tokens(&doc.content).len();

        if token_count > TOKEN_LIMIT {
            eprintln!(
                "    Document {}/{} ({} tokens) exceeds limit, chunking: {}",
                doc_index + 1,
                documents.len(),
                token_count,
                doc.path
            );

            let chunks = _chunk_content(&doc.content, &bpe, TOKEN_LIMIT - CHUNK_OVERLAP);
            let chunk_count = chunks.len();
            eprintln!("    Split into {} chunks", chunk_count);

            for (chunk_index, chunk) in chunks.into_iter().enumerate() {
                let chunk_path = if chunk_count > 1 {
                    format!("{} [chunk {}/{}]", doc.path, chunk_index + 1, chunk_count)
                } else {
                    doc.path.clone()
                };
                all_chunks.push((doc_index, chunk_path, chunk));
            }
        } else {
            all_chunks.push((doc_index, doc.path.clone(), doc.content.clone()));
        }
    }

    let total_chunks = all_chunks.len();
    eprintln!("Total chunks to process: {} (from {} documents)", total_chunks, documents.len());

    let results = stream::iter(all_chunks.into_iter().enumerate())
        .map(|(chunk_index, (_doc_index, path, content))| {
            // Clone client, model, and Arc<BPE> for the async block
            let client = client.clone();
            let model = model.to_string();
            let bpe = Arc::clone(&bpe); // Clone the Arc pointer
            let content_clone = content.clone(); // Clone content for returning

            async move {
                // Calculate token count for this chunk
                let token_count = bpe.encode_with_special_tokens(&content).len();

                // Prepare input for this chunk
                let inputs: Vec<String> = vec![content];

                let request = CreateEmbeddingRequestArgs::default()
                    .model(&model) // Use cloned model string
                    .input(inputs)
                    .build()?; // Propagates OpenAIError

                if chunk_index % 10 == 0 || chunk_index == total_chunks - 1 {
                    eprintln!(
                        "    Processing chunk {}/{} ({} tokens): {}",
                        chunk_index + 1,
                        total_chunks,
                        token_count,
                        path
                    );
                }
                let response = client.embeddings().create(request).await?; // Propagates OpenAIError

                if response.data.len() != 1 {
                    return Err(ServerError::OpenAI(
                        async_openai::error::OpenAIError::ApiError(OpenAIAPIErr {
                            message: format!(
                                "Mismatch in response length for chunk {}. Expected 1, got {}.",
                                chunk_index + 1, response.data.len()
                            ),
                            r#type: Some("sdk_error".to_string()),
                            param: None,
                            code: None,
                        }),
                    ));
                }

                // Process result
                let embedding_data = response.data.first().unwrap(); // Safe unwrap due to check above
                let embedding_array = Array1::from(embedding_data.embedding.clone());
                // Return successful embedding with path, content, and token count
                Ok((path, content_clone, embedding_array, token_count))
            }
        })
        .buffer_unordered(CONCURRENCY_LIMIT) // Run up to CONCURRENCY_LIMIT futures concurrently
        .collect::<Vec<Result<(String, String, Array1<f32>, usize), ServerError>>>() // Update collected result type
        .await;

    // Process collected results, filtering out errors and summing tokens
    let mut embeddings_vec = Vec::new();
    let mut total_processed_tokens: usize = 0;
    for result in results {
        match result {
            Ok((path, content, embedding, tokens)) => {
                embeddings_vec.push((path, content, embedding)); // Keep successful embeddings with content
                total_processed_tokens += tokens; // Add tokens for successful ones
            }
            Err(e) => {
                // Log error but potentially continue? Or return the first error?
                // For now, let's return the first error encountered.
                eprintln!("Error during concurrent embedding generation: {}", e);
                return Err(e);
            }
        }
    }

    eprintln!(
        "Finished generating embeddings. Successfully processed {} chunks/documents ({} tokens).",
        embeddings_vec.len(), total_processed_tokens
    );
    Ok((embeddings_vec, total_processed_tokens)) // Return tuple
}