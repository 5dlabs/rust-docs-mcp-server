use rustdocs_mcp_server::error::ServerError;
use rmcp::{
    ServerHandler,
    transport::io::stdio,
    service::{ServiceExt, RequestContext, RoleServer},
    model::{
        CallToolResult, Content,
        ListResourcesResult, ListPromptsResult,
        ListResourceTemplatesResult, ReadResourceResult, GetPromptResult,
        PaginatedRequestParam, ReadResourceRequestParam, GetPromptRequestParam,
        ProtocolVersion, ServerCapabilities, ServerInfo, Implementation,
        CallToolRequestParam,
    },
    Error as McpError,
};
use serde_json::json;
use std::env;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Simple wrapper that forwards MCP requests to HTTP server
#[derive(Clone)]
struct HttpWrapper {
    http_base_url: String,
}

impl HttpWrapper {
    fn new(http_base_url: String) -> Self {
        Self { http_base_url }
    }

    async fn forward_tool_call(&self, params: CallToolRequestParam) -> Result<CallToolResult, McpError> {
        // For now, we'll directly handle the query_rust_docs tool
        // In a full implementation, this would make HTTP requests to the backend
        if params.name == "query_rust_docs" {
            let args = params.arguments.unwrap_or_default();
            let crate_name = args.get("crate_name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let question = args.get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            // Make HTTP request to backend
            let client = reqwest::Client::new();
            let session_id = "wrapper-session";
            
            // First, establish SSE connection (in a real implementation)
            // For now, we'll simulate the response
            let response = match self.make_http_request(&client, session_id, crate_name, question).await {
                Ok(resp) => resp,
                Err(e) => return Err(McpError::internal_error(format!("HTTP request failed: {}", e), None)),
            };

            Ok(CallToolResult::success(vec![Content::text(response)]))
        } else {
            Err(McpError::invalid_request(format!("Unknown tool: {}", params.name), None))
        }
    }

    async fn make_http_request(
        &self,
        client: &reqwest::Client,
        _session_id: &str,
        crate_name: &str,
        question: &str,
    ) -> Result<String, ServerError> {
        // Create simple HTTP request to our API
        let request_body = json!({
            "crate_name": crate_name,
            "question": question
        });

        let response = client
            .post(format!("{}/query", self.http_base_url))
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ServerError::Internal(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ServerError::Internal(format!(
                "HTTP API error ({}): {}",
                status, error_text
            )));
        }

        let json: serde_json::Value = response.json().await
            .map_err(|e| ServerError::Internal(format!("Failed to parse JSON: {}", e)))?;
        
        // Extract the response
        if let Some(response_text) = json.get("response").and_then(|r| r.as_str()) {
            return Ok(response_text.to_string());
        }

        // If we couldn't parse the expected format, log the response for debugging
        eprintln!("Unexpected response format: {:?}", json);
        
        // Return error to see what's happening
        Err(ServerError::Internal(format!(
            "Failed to parse HTTP response. Got: {}",
            serde_json::to_string_pretty(&json).unwrap_or_else(|_| "unparseable".to_string())
        )))
    }
}

impl ServerHandler for HttpWrapper {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .build();

        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities,
            server_info: Implementation {
                name: "rust-docs-http-wrapper".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            instructions: Some("HTTP wrapper for Rust documentation MCP server. Forwards requests to HTTP backend.".to_string()),
        }
    }

    async fn list_resources(
        &self,
        _request: PaginatedRequestParam,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![],
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        _request: ReadResourceRequestParam,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        Err(McpError::invalid_request("No resources available".to_string(), None))
    }

    async fn list_prompts(
        &self,
        _request: PaginatedRequestParam,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult {
            prompts: vec![],
            next_cursor: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParam,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        Err(McpError::invalid_params(
            format!("Prompt not found: {}", request.name),
            None,
        ))
    }

    async fn list_resource_templates(
        &self,
        _request: PaginatedRequestParam,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![],
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        params: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.forward_tool_call(params).await
    }

    async fn list_tools(
        &self,
        _request: PaginatedRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, McpError> {
        Ok(rmcp::model::ListToolsResult {
            tools: vec![rmcp::model::Tool {
                name: "query_rust_docs".to_string().into(),
                description: "Query documentation for a specific Rust crate using semantic search and LLM summarization.".to_string().into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "crate_name": {
                            "type": "string",
                            "description": "The crate to search in (e.g., \"axum\", \"tokio\", \"serde\")"
                        },
                        "question": {
                            "type": "string", 
                            "description": "The specific question about the crate's API or usage."
                        }
                    },
                    "required": ["crate_name", "question"]
                }).as_object().unwrap().clone().into(),
            }],
            next_cursor: None,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), ServerError> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mcp_http_wrapper=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("🚀 Starting MCP HTTP Wrapper");

    // Get HTTP backend URL from environment or use default
    let http_base_url = env::var("MCP_HTTP_URL")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());

    info!("📡 Forwarding to HTTP backend: {}", http_base_url);

    // Create the wrapper
    let wrapper = HttpWrapper::new(http_base_url);

    // Use stdio transport
    let stdio_transport = stdio();
    
    info!("🔧 Using stdio transport for MCP communication");

    // Serve the wrapper
    match wrapper.serve(stdio_transport).await {
        Ok(service) => {
            info!("✅ MCP HTTP wrapper started successfully");
            if let Err(e) = service.waiting().await {
                eprintln!("Service error: {}", e);
            }
        }
        Err(e) => {
            eprintln!("Failed to start wrapper: {}", e);
            return Err(ServerError::Internal(format!("Wrapper startup failed: {}", e)));
        }
    }

    Ok(())
}