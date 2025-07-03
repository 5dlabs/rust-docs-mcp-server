#!/bin/bash

# Script to backfill crates that have too few documents (1-3 docs)

echo "🔄 Backfill Script for Under-documented Crates"
echo "This will re-populate crates that have 3 or fewer documents"
echo ""

# Create log directory
mkdir -p logs
LOG_FILE="logs/backfill_$(date +%Y%m%d_%H%M%S).log"

# Get list of under-documented crates
echo "🔍 Finding crates with 3 or fewer documents..."
UNDERDOC_CRATES=$(cargo run --bin populate_db -- --list 2>/dev/null | awk '$3 <= 3 && NR > 1 {print $1}' | grep -v "^---")

# Count them
TOTAL_COUNT=$(echo "$UNDERDOC_CRATES" | wc -l | xargs)
echo "📊 Found $TOTAL_COUNT crates that need backfilling:"
echo "$UNDERDOC_CRATES" | nl -w2 -s'. '
echo ""

# Ask for confirmation
read -p "🤔 Do you want to proceed with backfilling these $TOTAL_COUNT crates? (y/N) " -n 1 -r
echo ""
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "❌ Backfill cancelled"
    exit 0
fi

echo ""
echo "🚀 Starting backfill process..."
echo "📝 Full log: $LOG_FILE"
echo ""

# Process each crate
CURRENT=0
SUCCESS=0
FAILED=0
TIMEOUT=0

for CRATE_NAME in $UNDERDOC_CRATES; do
    CURRENT=$((CURRENT + 1))
    echo "════════════════════════════════════════════════════════" | tee -a "$LOG_FILE"
    echo "📦 [$CURRENT/$TOTAL_COUNT] Backfilling: $CRATE_NAME" | tee -a "$LOG_FILE"
    echo "════════════════════════════════════════════════════════" | tee -a "$LOG_FILE"
    
    # Get current doc count
    OLD_COUNT=$(cargo run --bin populate_db -- --list 2>/dev/null | grep "^$CRATE_NAME " | awk '{print $3}')
    echo "  Current docs: $OLD_COUNT" | tee -a "$LOG_FILE"
    
    # Get features for this crate from proxy-config.json
    FEATURES=$(jq -r --arg name "$CRATE_NAME" '.crates[] | select(.name == $name) | .features | if . then join(",") else "" end' proxy-config.json 2>/dev/null)
    
    # Build command with --force flag
    if [ -n "$FEATURES" ]; then
        echo "  Features: $FEATURES" | tee -a "$LOG_FILE"
        CMD="cargo run --bin populate_db -- --crate-name $CRATE_NAME --features $FEATURES --force"
    else
        CMD="cargo run --bin populate_db -- --crate-name $CRATE_NAME --force"
    fi
    
    echo "  Running: $CMD" | tee -a "$LOG_FILE"
    echo "" | tee -a "$LOG_FILE"
    
    # Run with timeout of 10 minutes per crate
    timeout 600 $CMD >> "$LOG_FILE" 2>&1
    EXIT_CODE=$?
    
    if [ $EXIT_CODE -eq 0 ]; then
        # Get new doc count
        NEW_COUNT=$(cargo run --bin populate_db -- --list 2>/dev/null | grep "^$CRATE_NAME " | awk '{print $3}')
        echo "✅ Successfully backfilled $CRATE_NAME ($OLD_COUNT → $NEW_COUNT docs)" | tee -a "$LOG_FILE"
        SUCCESS=$((SUCCESS + 1))
    elif [ $EXIT_CODE -eq 124 ]; then
        echo "⏱️  Timed out backfilling $CRATE_NAME (after 10 minutes)" | tee -a "$LOG_FILE"
        TIMEOUT=$((TIMEOUT + 1))
    else
        echo "❌ Failed to backfill $CRATE_NAME (exit code: $EXIT_CODE)" | tee -a "$LOG_FILE"
        FAILED=$((FAILED + 1))
    fi
    
    echo "" | tee -a "$LOG_FILE"
    
    # Small delay between crates to be respectful to docs.rs
    if [ $CURRENT -lt $TOTAL_COUNT ]; then
        echo "⏳ Waiting 3 seconds before next crate..." | tee -a "$LOG_FILE"
        sleep 3
        echo ""
    fi
done

echo "════════════════════════════════════════════════════════" | tee -a "$LOG_FILE"
echo "📊 Backfill Summary" | tee -a "$LOG_FILE"
echo "════════════════════════════════════════════════════════" | tee -a "$LOG_FILE"
echo "  Total crates: $TOTAL_COUNT" | tee -a "$LOG_FILE"
echo "  ✅ Success: $SUCCESS" | tee -a "$LOG_FILE"
echo "  ❌ Failed: $FAILED" | tee -a "$LOG_FILE"
echo "  ⏱️  Timeout: $TIMEOUT" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

# Show current status
echo "📈 Current database status:" | tee -a "$LOG_FILE"
TOTAL_CRATES=$(cargo run --bin populate_db -- --list 2>/dev/null | tail -n +2 | wc -l | xargs)
WELL_DOC=$(cargo run --bin populate_db -- --list 2>/dev/null | awk '$3 > 3 && NR > 1' | wc -l | xargs)
echo "  Total crates: $TOTAL_CRATES" | tee -a "$LOG_FILE"
echo "  Well-documented (>3 docs): $WELL_DOC" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

echo "✅ Backfill complete! Check $LOG_FILE for full details." | tee -a "$LOG_FILE"