# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Rust-based MCP (Model Context Protocol) server that provides AI assistants with up-to-date Rust crate documentation. It uses PostgreSQL with pgvector for semantic search capabilities and supports multiple embedding providers (OpenAI and Voyage AI).

The project transitioned from a single-crate local cache system to a scalable PostgreSQL-based solution that can handle multiple crates with efficient vector similarity search.

## Essential Commands

### Building and Testing
```bash
# Build the project
cargo build --release

# Check for compilation errors
cargo check

# Run tests (if any exist)
cargo test

# Run a specific binary
cargo run --bin rustdocs_mcp_server -- [args]
cargo run --bin populate_db -- [args]
cargo run --bin populate_all
cargo run --bin backfill_versions
```

### Database Setup
```bash
# Create database with pgvector extension
createdb rust_docs_vectors
psql rust_docs_vectors < schema.sql

# Set required environment variables
export MCPDOCS_DATABASE_URL="postgresql://username@localhost/rust_docs_vectors"
export OPENAI_API_KEY="sk-..." # Or VOYAGE_API_KEY for Voyage embeddings
```

### Populating Documentation
```bash
# Populate a single crate
cargo run --bin populate_db -- --crate-name tokio --features full

# Populate all crates from proxy-config.json
cargo run --bin populate_all

# List populated crates
cargo run --bin populate_db -- --list

# Update version information
cargo run --bin backfill_versions
```

### Running the MCP Server
```bash
# Run with specific crates
cargo run --bin rustdocs_mcp_server tokio serde

# Run with all available crates
cargo run --bin rustdocs_mcp_server --all

# Specify embedding provider
cargo run --bin rustdocs_mcp_server --embedding-provider voyage --embedding-model voyage-3 tokio
```

## Architecture

### Core Components

1. **Database Layer** (`database.rs`): 
   - PostgreSQL interface using SQLx with pgvector extension
   - Handles crate metadata and document embeddings storage
   - Implements vector similarity search with IVFFlat indexing

2. **Document Loading** (`doc_loader.rs`):
   - Parses HTML documentation from `cargo doc` output
   - Extracts and chunks documentation content
   - Handles crate feature specifications

3. **Embeddings** (`embeddings.rs`):
   - Supports OpenAI (text-embedding-3-small) and Voyage AI (voyage-3) providers
   - Generates 1536-dimensional embeddings for documents
   - Handles token counting and API interactions

4. **MCP Server** (`server.rs`):
   - Implements the Model Context Protocol using rmcp
   - Exposes `query_rust_docs` tool for semantic search
   - Manages server state and request handling

5. **Error Handling** (`error.rs`):
   - Custom `ServerError` type with thiserror
   - Comprehensive error propagation throughout the system

### Binary Tools

- **rustdocs_mcp_server**: Main MCP server serving documentation queries
- **populate_db**: Populates database with single crate documentation
- **populate_all**: Batch populates multiple crates from configuration
- **backfill_versions**: Updates version information for existing crates

### Database Schema

The PostgreSQL database requires the pgvector extension and includes:
- `crates` table: Stores crate metadata (name, version, doc stats)
- `doc_embeddings` table: Stores document chunks with vector embeddings
- `search_similar_docs` function: Performs vector similarity search
- IVFFlat index on embeddings for performance

### Configuration Files

- `proxy-config.json`: Lists crates to populate with their required features
- `mcp-config.json` / `claude-desktop-config.json`: MCP client configurations
- Environment variables control database connection and API keys

### Environment Variables

- `MCPDOCS_DATABASE_URL` - PostgreSQL connection string for the rust docs database
- `OPENAI_API_KEY` - OpenAI API key (if using OpenAI embeddings)
- `VOYAGE_API_KEY` - Voyage AI API key (if using Voyage embeddings)
- `RUST_LOG` - Logging level configuration

## Development Notes

- The project uses async/await patterns throughout with Tokio runtime
- All database operations are async using SQLx
- Error handling uses Result types with custom ServerError
- The MCP protocol implementation uses the rmcp crate
- Release builds are optimized for size with LTO, strip, and panic=abort