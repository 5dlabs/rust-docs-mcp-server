#!/bin/bash

# Test the stdio MCP server
{
  echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"test","version":"1.0"}}}'
  sleep 0.1
  echo '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
  sleep 0.1
  echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"query_rust_docs","arguments":{"crate_name":"tokio","question":"How do I use spawn?"}}}'
  sleep 1
} | /Users/jonathonfritz/rust-docs-mcp-server/target/release/rustdocs_mcp_server --all 2>&1 | grep -E "(jsonrpc|error|result)" | head -20