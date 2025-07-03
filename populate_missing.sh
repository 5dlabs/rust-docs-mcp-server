#!/bin/bash

# Script to populate missing crates

echo "🔧 Populating missing crates from proxy-config.json"
echo ""

# Array of missing crates
MISSING_CRATES=(
    "cargo-udeps"
    "futures-util"
    "mcp-core"
    "opentelemetry_sdk"
    "port-check"
    "solana-sdk"
    "sqlx-postgres"
    "tokio-stream"
    "tokio-test"
    "tokio-tungstenite"
    "tonic-build"
    "tonic-reflection"
    "tonic-web"
    "tower-http"
)

# Get features from proxy-config.json for each crate
for CRATE in "${MISSING_CRATES[@]}"; do
    echo "════════════════════════════════════════════════════════"
    echo "📦 Processing: $CRATE"
    echo "════════════════════════════════════════════════════════"
    
    # Get features for this crate
    FEATURES=$(jq -r --arg name "$CRATE" '.crates[] | select(.name == $name) | .features | if . then join(",") else "" end' proxy-config.json)
    
    # Build command
    if [ -n "$FEATURES" ]; then
        echo "  Features: $FEATURES"
        CMD="cargo run --bin populate_db -- --crate-name $CRATE --features $FEATURES --force"
    else
        CMD="cargo run --bin populate_db -- --crate-name $CRATE --force"
    fi
    
    echo "  Running: $CMD"
    echo ""
    
    # Run the command
    $CMD
    
    EXIT_CODE=$?
    if [ $EXIT_CODE -eq 0 ]; then
        echo "✅ Successfully populated $CRATE"
    else
        echo "❌ Failed to populate $CRATE (exit code: $EXIT_CODE)"
    fi
    
    echo ""
    
    # Small delay between crates
    sleep 2
done

echo "════════════════════════════════════════════════════════"
echo "✅ Finished processing missing crates"
echo ""

# Show final status
echo "📊 Final database status:"
cargo run --bin populate_db -- --list 2>/dev/null | head -5
TOTAL=$(psql -d rust_docs_vectors -t -c "SELECT COUNT(DISTINCT crate_name) FROM doc_embeddings;" | xargs)
echo ""
echo "Total crates in database: $TOTAL"