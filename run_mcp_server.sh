#!/bin/bash

# Load environment variables from .env if it exists
if [ -f /Users/jonathonfritz/rust-docs-mcp-server/.env ]; then
    export $(grep -v '^#' /Users/jonathonfritz/rust-docs-mcp-server/.env | xargs)
fi

# Ensure required environment variables are set
if [ -z "$MCPDOCS_DATABASE_URL" ]; then
    echo "Error: MCPDOCS_DATABASE_URL not set" >&2
    exit 1
fi

if [ -z "$OPENAI_API_KEY" ]; then
    echo "Error: OPENAI_API_KEY not set" >&2
    exit 1
fi

# Run the server
exec /Users/jonathonfritz/rust-docs-mcp-server/target/release/rustdocs_mcp_server --all