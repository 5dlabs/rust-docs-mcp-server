# SQL Schema and Migrations

This directory contains the database schema and migration files for the rust-docs-mcp-server.

## Files

- `schema.sql` - Main database schema with pgvector extension for 3072-dimensional embeddings
- `migrations/` - Historical migration files

## Setup

To set up a new database:

```bash
# Create database with pgvector extension
createdb rust_docs_vectors
psql rust_docs_vectors < sql/schema.sql
```

## Schema Overview

The database uses PostgreSQL with the pgvector extension to store document embeddings:

- **crates** table: Stores crate metadata (name, version, timestamps)
- **doc_embeddings** table: Stores document chunks with their vector embeddings
- **crate_stats** view: Provides statistics for each crate
- **search_similar_docs** function: Performs vector similarity search using cosine distance

The embeddings are 3072-dimensional vectors from OpenAI's text-embedding-3-large model.