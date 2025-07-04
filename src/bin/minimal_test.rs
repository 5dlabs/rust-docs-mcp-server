// Minimal reproduction test - exact same setup as our HTTP server but simplified
use axum::{
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde_json::json;
use std::net::SocketAddr;
use tokio::net::TcpListener;

// Test functions - identical signatures to our main server
async fn test_health() -> impl IntoResponse {
    println!("test_health called!");
    Json(json!({
        "status": "healthy",
        "test": "minimal"
    }))
}

async fn test_ready() -> impl IntoResponse {
    println!("test_ready called!");
    Json(json!({
        "status": "ready", 
        "test": "minimal"
    }))
}

async fn test_info() -> impl IntoResponse {
    println!("test_info called!");
    Json(json!({
        "service": "test",
        "test": "minimal"
    }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Starting minimal reproduction test");
    
    // Create router - exact same pattern as our main server
    let app = Router::new()
        .route("/health", get(test_health))
        .route("/ready", get(test_ready))
        .route("/info", get(test_info));

    let addr: SocketAddr = "0.0.0.0:8081".parse()?;
    let listener = TcpListener::bind(&addr).await?;
    
    println!("🌐 Test server listening on http://{}", addr);
    println!("Routes:");
    println!("  GET /health");
    println!("  GET /ready"); 
    println!("  GET /info");
    println!("📝 Testing with curl:");
    println!("  curl http://localhost:8081/health");
    println!("  curl http://localhost:8081/ready");
    println!("  curl http://localhost:8081/info");

    // Test both axum::serve and alternative approaches
    println!("🚀 Using axum::serve...");
    axum::serve(listener, app).await?;
    
    Ok(())
}