#!/bin/bash

# Script to monitor populate_all progress and handle timeouts gracefully

echo "🚀 Starting batch population monitor..."
echo "📊 This script will:"
echo "  - Run populate_all and capture output"
echo "  - Show real-time progress"
echo "  - Continue even if the process times out"
echo ""

# Create log directory
mkdir -p logs
LOG_FILE="logs/populate_$(date +%Y%m%d_%H%M%S).log"

# Function to check database status
check_db_status() {
    echo "📈 Current database status:"
    cargo run --bin populate_db -- --list | tail -n +2 | wc -l | xargs -I {} echo "  Total crates populated: {}"
    echo ""
}

# Show initial status
check_db_status

# Run populate_all with output to both terminal and log file
echo "🔄 Running populate_all (this may take a while)..."
echo "📝 Full log: $LOG_FILE"
echo ""

# Run with timeout of 30 minutes and capture output
timeout 1800 cargo run --bin populate_all 2>&1 | tee "$LOG_FILE" | while IFS= read -r line; do
    # Show key status lines
    if [[ $line == *"✅"* ]] || [[ $line == *"❌"* ]] || [[ $line == *"📥"* ]] || [[ $line == *"🧠"* ]] || [[ $line == *"Processing page"* ]]; then
        echo "$line"
    elif [[ $line == *"Generated"* ]] && [[ $line == *"embeddings"* ]]; then
        echo "  ✅ $line"
    elif [[ $line == *"Loaded"* ]] && [[ $line == *"documents"* ]]; then
        echo "  📄 $line"
    fi
done

EXIT_CODE=$?

echo ""
if [ $EXIT_CODE -eq 124 ]; then
    echo "⏱️  Process timed out after 30 minutes (this is normal for large batches)"
else
    echo "✅ Process completed with exit code: $EXIT_CODE"
fi

# Show final status
echo ""
check_db_status

# Check for remaining crates
echo "🔍 Checking for remaining crates to populate..."
REMAINING=$(grep -c "❌.*needs to be populated" "$LOG_FILE" 2>/dev/null || echo "0")
echo "  Remaining crates: $REMAINING"

if [ "$REMAINING" -gt 0 ]; then
    echo ""
    echo "💡 Tip: Run this script again to continue populating remaining crates"
    echo "   Or use: ./populate_individual.sh to process them one by one"
fi

echo ""
echo "📊 Summary from log:"
grep -E "(Generated|embeddings for|Loaded.*documents)" "$LOG_FILE" | tail -20

echo ""
echo "✅ Monitor complete. Check $LOG_FILE for full details."