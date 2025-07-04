# Rust Docs MCP Server

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A high-performance MCP (Model Context Protocol) server that provides semantic search across Rust crate documentation using PostgreSQL with pgvector for vector similarity search.

## Features

- **PostgreSQL Vector Database**: Uses pgvector extension for efficient vector similarity search
- **Multi-Crate Support**: Search across multiple Rust crates in a single server instance
- **Semantic Search**: Uses OpenAI's `text-embedding-3-large` model (3072 dimensions) for accurate retrieval
- **LLM Integration**: Leverages OpenAI's `gpt-4o-mini-2024-07-18` for context-aware responses
- **Efficient Pagination**: Handles large crates with configurable page limits
- **Database-Driven**: No memory loading - all search operations query the database directly

## Architecture

```
┌─────────────────┐     ┌──────────────────┐
│   MCP Client    │────▶│  rust-docs-mcp   │
│ (Claude/Cursor) │     │     Server       │
└─────────────────┘     └────────┬─────────┘
                                 │
                                 │ Vector Search
                                 ▼
                        ┌─────────────────┐
                        │   PostgreSQL    │
                        │   + pgvector    │
                        │                 │
                        │ 38+ crates      │
                        │ 1,185+ docs     │
                        │ 6M+ tokens      │
                        └─────────────────┘
```

## Prerequisites

- **PostgreSQL with pgvector extension**: For vector similarity search
- **OpenAI API Key**: For embeddings and LLM responses
- **Rust toolchain**: For building from source

## Installation & Setup

### 1. Database Setup

```bash
# Install PostgreSQL and pgvector (example for macOS with Homebrew)
brew install postgresql@17
brew install pgvector

# Start PostgreSQL
brew services start postgresql@17

# Create database
createdb rust_docs_vectors
psql rust_docs_vectors -c "CREATE EXTENSION IF NOT EXISTS vector;"

# Apply schema
psql rust_docs_vectors < sql/schema.sql
```

### 2. Environment Variables

```bash
export MCPDOCS_DATABASE_URL="postgresql://username@localhost/rust_docs_vectors"
export OPENAI_API_KEY="sk-..."
export LLM_MODEL="gpt-4o-mini-2024-07-18"  # Optional
export EMBEDDING_MODEL="text-embedding-3-large"  # Optional
```

### 3. Build the Server

```bash
git clone https://github.com/your-repo/rust-docs-mcp-server.git
cd rust-docs-mcp-server
cargo build --release
```

## Usage

### 1. Populate Documentation Database

Populate all crates from the configuration:
```bash
cargo run --bin populate_all
```

Or populate individual crates:
```bash
cargo run --bin populate_db -- --crate-name tokio --features full --max-pages 100
cargo run --bin populate_db -- --crate-name serde --features derive
```

### 2. Run the MCP Server

```bash
# Serve all available crates
cargo run --bin rustdocs_mcp_server -- --all

# Serve specific crates
cargo run --bin rustdocs_mcp_server -- tokio serde axum

# List available crates
cargo run --bin rustdocs_mcp_server -- --list
```

### 3. MCP Tool Usage

The server exposes a `query_rust_docs` tool:

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "query_rust_docs",
    "arguments": {
      "crate_name": "tokio",
      "question": "How do I create a TCP server?"
    }
  },
  "id": 1
}
```

## Configuration Files

### proxy-config.json
Defines which crates to populate and their required features:

```json
{
  "rustdocs_binary_path": "./target/release/rustdocs_mcp_server",
  "crates": [
    {
      "name": "tokio",
      "features": ["full"],
      "enabled": true
    },
    {
      "name": "serde",
      "features": ["derive"],
      "enabled": true
    }
  ]
}
```

## CLI Tools

### Core Binaries
- **`rustdocs_mcp_server`** - Main MCP server
- **`populate_db`** - Populate single crate documentation
- **`populate_all`** - Batch populate from proxy-config.json
- **`backfill_versions`** - Update version information

### Database Management
```bash
# List populated crates
cargo run --bin populate_db -- --list

# Delete a crate's documentation
cargo run --bin populate_db -- --delete tokio

# Force repopulation
cargo run --bin populate_db -- --crate-name tokio --force
```

## Client Configuration

### Claude Desktop

Add to your Claude Desktop MCP configuration:

```json
{
  "mcpServers": {
    "rust-docs": {
      "command": "/path/to/rustdocs_mcp_server",
      "args": ["--all"],
      "env": {
        "MCPDOCS_DATABASE_URL": "postgresql://username@localhost/rust_docs_vectors",
        "OPENAI_API_KEY": "sk-..."
      }
    }
  }
}
```

### Cursor IDE

Configure as an MCP server in your Cursor settings for enhanced Rust development.

## Database Schema

The system uses three main tables:

- **`crates`**: Stores crate metadata (name, version, statistics)
- **`doc_embeddings`**: Stores document chunks with 3072-dimensional embeddings
- **`crate_stats`**: View providing aggregated statistics per crate

Vector similarity search uses cosine distance with the pgvector extension.

## Performance

- **Database-driven**: No memory loading of embeddings
- **Efficient search**: Vector similarity search with PostgreSQL indexes
- **Scalable**: Can handle dozens of crates with thousands of documents
- **Fast startup**: Server starts immediately, queries database on demand

## Supported Crates

The server works with any Rust crate available on docs.rs. Popular crates include:

- **Web**: axum, reqwest, hyper, tower, tower-http
- **Async**: tokio, futures, async-trait
- **Serialization**: serde, serde_json, toml
- **CLI**: clap, tracing, anyhow, thiserror
- **Database**: sqlx, redis
- **And many more...**

## Development

### Adding New Crates

1. Add to `proxy-config.json`
2. Run `cargo run --bin populate_all`
3. Verify with `cargo run --bin populate_db -- --list`

### Debugging

- Check database connection: `psql rust_docs_vectors -c "SELECT COUNT(*) FROM crates;"`
- View server logs for query processing details
- Use `--verbose` flags for detailed output

## Architecture Notes

- **Vector Embeddings**: 3072-dimensional from OpenAI's text-embedding-3-large
- **Database**: PostgreSQL 12+ with pgvector extension
- **Search**: Cosine similarity with configurable result limits
- **Caching**: Database-persistent, no local file caching
- **Concurrency**: Async Rust with tokio runtime

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Support

For issues and questions:
- GitHub Issues: Report bugs and feature requests
- Documentation: Check the `sql/README.md` for database details
- Architecture: See `ARCHITECTURE_PLAN.md` for system design