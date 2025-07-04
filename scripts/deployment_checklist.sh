#!/bin/bash

# Deployment Checklist for rust-docs-mcp-server
# This script helps verify all components are ready for Kubernetes deployment

echo "🔍 Rust Docs MCP Server - Deployment Readiness Check"
echo "===================================================="
echo ""

# Color codes
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Check functions
check_pass() {
    echo -e "${GREEN}✅ $1${NC}"
}

check_fail() {
    echo -e "${RED}❌ $1${NC}"
}

check_warn() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

echo "1. Code Readiness"
echo "-----------------"

# Check if memory loading is fixed
if grep -q "db.get_crate_documents" src/main.rs; then
    check_fail "main.rs still loads embeddings into memory"
else
    check_pass "main.rs memory loading fixed (needs verification)"
fi

# Check if HTTP server is implemented
if [ -f "src/http_server.rs" ]; then
    check_pass "HTTP server module exists"
else
    check_fail "HTTP server module missing"
fi

# Check if preview tool works
if cargo check --bin preview_crate 2>/dev/null; then
    check_pass "Preview tool compiles"
else
    check_fail "Preview tool has compilation errors"
fi

echo ""
echo "2. Database Status"
echo "------------------"

# Check database connection
if psql rust_docs_vectors -c "SELECT COUNT(*) FROM crates;" &>/dev/null; then
    CRATE_COUNT=$(psql rust_docs_vectors -t -c "SELECT COUNT(*) FROM crates;" | xargs)
    check_pass "Database connected: $CRATE_COUNT crates loaded"
else
    check_fail "Cannot connect to database"
fi

echo ""
echo "3. Environment Variables"
echo "------------------------"

# Check required env vars
[ -n "$MCPDOCS_DATABASE_URL" ] && check_pass "MCPDOCS_DATABASE_URL set" || check_fail "MCPDOCS_DATABASE_URL not set"
[ -n "$OPENAI_API_KEY" ] && check_pass "OPENAI_API_KEY set" || check_fail "OPENAI_API_KEY not set"

echo ""
echo "4. Docker Readiness"
echo "-------------------"

# Check if Dockerfile exists
if [ -f "Dockerfile" ]; then
    check_pass "Dockerfile exists"
else
    check_warn "Dockerfile not created yet"
fi

# Check if docker is installed
if command -v docker &>/dev/null; then
    check_pass "Docker installed"
else
    check_fail "Docker not installed"
fi

echo ""
echo "5. Kubernetes Readiness"
echo "-----------------------"

# Check if kubectl is configured
if command -v kubectl &>/dev/null; then
    if kubectl cluster-info &>/dev/null; then
        CLUSTER=$(kubectl config current-context)
        check_pass "kubectl configured: $CLUSTER"
    else
        check_fail "kubectl not connected to cluster"
    fi
else
    check_fail "kubectl not installed"
fi

# Check for k8s manifests
if [ -d "k8s" ] || [ -d "manifests" ] || [ -d "helm" ]; then
    check_pass "Kubernetes manifests directory exists"
else
    check_warn "No Kubernetes manifests directory found"
fi

echo ""
echo "6. Build & Test"
echo "----------------"

# Check if release build works
if cargo build --release --bin rustdocs_mcp_server 2>/dev/null; then
    check_pass "Release build successful"
else
    check_fail "Release build failed"
fi

echo ""
echo "📋 Summary"
echo "----------"
echo "Review the checklist above and fix any ❌ items before deployment."
echo ""
echo "Next steps:"
echo "1. Fix any failing checks"
echo "2. Create Dockerfile if missing"
echo "3. Create Kubernetes manifests"
echo "4. Test locally with docker-compose"
echo "5. Deploy to Kubernetes cluster"