#!/bin/bash

echo "🔍 Rust Docs MCP Server Status Report"
echo "====================================="
echo ""

# Database info
echo "📊 Database Status:"
echo "  Database: rust_docs_vectors"
echo "  Embedding Model: text-embedding-3-large (3072 dimensions)"
echo ""

# Crate statistics
TOTAL_CRATES=$(psql -d rust_docs_vectors -t -c "SELECT COUNT(DISTINCT name) FROM crates;" | xargs)
TOTAL_DOCS=$(psql -d rust_docs_vectors -t -c "SELECT COUNT(*) FROM doc_embeddings;" | xargs)
TOTAL_TOKENS=$(psql -d rust_docs_vectors -t -c "SELECT SUM(token_count) FROM doc_embeddings;" | xargs)

echo "📦 Crate Statistics:"
echo "  Total crates loaded: $TOTAL_CRATES / 60"
echo "  Total documents: $TOTAL_DOCS"
echo "  Total tokens: $TOTAL_TOKENS"
echo ""

echo "📋 Top 10 Crates by Document Count:"
psql -d rust_docs_vectors -t -c "SELECT name, doc_count FROM crate_stats ORDER BY doc_count DESC LIMIT 10;" | column -t | sed 's/^/  /'
echo ""

echo "⚠️  Missing Crates (14):"
echo "  These crates failed to populate due to timeout issues:"
echo "  - cargo-udeps"
echo "  - futures-util" 
echo "  - mcp-core"
echo "  - opentelemetry_sdk"
echo "  - port-check"
echo "  - solana-sdk"
echo "  - sqlx-postgres"
echo "  - tokio-stream"
echo "  - tokio-test"
echo "  - tokio-tungstenite"
echo "  - tonic-build"
echo "  - tonic-reflection"
echo "  - tonic-web"
echo "  - tower-http"
echo ""

echo "✅ Server Configuration:"
echo "  Default embedding provider: openai"
echo "  Default embedding model: text-embedding-3-large"
echo "  Vector dimensions: 3072"
echo "  Environment variable: MCPDOCS_DATABASE_URL"
echo ""

echo "🚀 To run the server:"
echo "  cargo run --bin rustdocs_mcp_server -- --all"
echo "  cargo run --bin rustdocs_mcp_server -- tokio axum serde"