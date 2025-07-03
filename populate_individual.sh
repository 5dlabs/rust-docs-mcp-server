#!/bin/bash

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

echo "🔧 Populating missing crates individually"
echo ""

for CRATE in "${MISSING_CRATES[@]}"; do
    echo "════════════════════════════════════════════════════════"
    echo "📦 Processing: $CRATE"
    echo "════════════════════════════════════════════════════════"
    
    # Get features for this crate
    FEATURES=$(jq -r --arg name "$CRATE" '.crates[] | select(.name == $name) | .features | if . then join(",") else "" end' proxy-config.json)
    
    # Build command
    if [ -n "$FEATURES" ]; then
        echo "  Features: $FEATURES"
        CMD="cargo run --release --bin populate_db -- --crate-name $CRATE --features $FEATURES --force"
    else
        CMD="cargo run --release --bin populate_db -- --crate-name $CRATE --force"
    fi
    
    echo "  Running: $CMD"
    echo ""
    
    # Run the command with timeout
    timeout 300 $CMD
    
    EXIT_CODE=$?
    if [ $EXIT_CODE -eq 0 ]; then
        echo "✅ Successfully populated $CRATE"
    elif [ $EXIT_CODE -eq 124 ]; then
        echo "⏱️  Timed out after 5 minutes for $CRATE"
    else
        echo "❌ Failed to populate $CRATE (exit code: $EXIT_CODE)"
    fi
    
    echo ""
done

echo "════════════════════════════════════════════════════════"
echo "✅ Finished processing missing crates"
echo ""

# Show final status
echo "📊 Final database status:"
TOTAL=$(psql -d rust_docs_vectors -t -c "SELECT COUNT(DISTINCT name) FROM crates;" | xargs)
echo "Total crates in database: $TOTAL"