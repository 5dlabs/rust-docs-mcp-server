# Architecture Plan for rust-docs-mcp-server

## Current Status

### ✅ What's Working
- Database vector search is implemented in `server.rs` (line 263: `self.database.search_similar_docs()`)
- PostgreSQL with pgvector is set up and populated with 38 crates
- Pagination feature allows populating large crates without timeouts

### ❌ Issues Found
1. **Memory Loading Issue**: `main.rs` still loads ALL embeddings into memory at startup (lines 184-217)
   - This defeats the purpose of database search
   - Causes slow startup and high memory usage
   - Need to remove this and only keep database connection

## Required Changes

### 1. Fix Database-Only Mode
- Remove embedding loading from `main.rs`
- Server should only:
  - Connect to database
  - Initialize embedding provider for query embedding generation
  - Pass database connection to server

### 2. Add Document Count Preview Feature
Create a new binary `preview_crate` that:
- Scrapes docs.rs to count available pages/documents
- Estimates documentation size before populating
- Helps determine appropriate `max_pages` setting

### 3. Add HTTP Server Mode (MCP-Compliant)
Create dual-mode server that maintains MCP protocol compliance:
- **MCP Mode** (current): stdin/stdout JSON-RPC for Claude desktop
- **HTTP Mode** (new): HTTP transport for MCP JSON-RPC messages

The HTTP server will accept MCP protocol messages over HTTP:
```
POST /mcp
Content-Type: application/json

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

Additional endpoints for Kubernetes:
```
GET /health        # Liveness probe
GET /ready         # Readiness probe (checks DB connection)
GET /metrics       # Prometheus metrics
```

This ensures compatibility with any MCP client while enabling cloud deployment.

### 4. Containerization Strategy

#### Dockerfile Structure
```dockerfile
# Build stage
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin rustdocs_mcp_server

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/rustdocs_mcp_server /usr/local/bin/
CMD ["rustdocs_mcp_server", "--http-mode"]
```

### 5. Kubernetes Architecture

```yaml
# Components needed:
1. PostgreSQL with pgvector (using Bitnami chart + custom image)
2. rust-docs-server Deployment
3. ConfigMap for proxy-config.json
4. Secrets for API keys
5. Service for HTTP exposure
6. CronJob for periodic doc updates
```

### 6. Environment Variables
```bash
# Server config
MCPDOCS_DATABASE_URL=postgresql://user:pass@postgres:5432/rust_docs_vectors
OPENAI_API_KEY=sk-...
LLM_MODEL=gpt-4o-mini-2024-07-18
SERVER_MODE=http  # or 'mcp'
HTTP_PORT=8080

# Feature flags
ENABLE_METRICS=true
ENABLE_TRACING=true
```

## Implementation Order

1. **Phase 1: Fix Memory Loading** (Quick fix)
   - Modify main.rs to not load embeddings
   - Test server still works with DB-only mode

2. **Phase 2: Add Preview Tool** (1-2 hours)
   - Create preview_crate binary
   - Add to populate workflow

3. **Phase 3: HTTP Server Mode** (4-6 hours)
   - Add HTTP mode to main.rs
   - Implement REST endpoints
   - Add OpenAPI documentation

4. **Phase 4: Containerization** (2-3 hours)
   - Create multi-stage Dockerfile
   - Add docker-compose for local testing
   - Create build scripts

5. **Phase 5: Kubernetes Deployment** (4-6 hours)
   - Create Helm chart
   - Set up PostgreSQL with pgvector
   - Configure secrets and configmaps
   - Add monitoring/logging

## Benefits

1. **Scalability**: Can handle multiple instances behind load balancer
2. **Efficiency**: No memory loading, pure database queries
3. **Flexibility**: Works in both local (MCP) and cloud (HTTP) modes
4. **Maintainability**: Easy updates via Kubernetes rolling deployments
5. **Observability**: Health checks, metrics, distributed tracing