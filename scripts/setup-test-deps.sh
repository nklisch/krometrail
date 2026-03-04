#!/usr/bin/env bash
set -euo pipefail

# Idempotent script to check/install debuggers needed for integration and e2e tests.

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

ok()   { echo -e "  ${GREEN}✓${NC} $1"; }
warn() { echo -e "  ${YELLOW}!${NC} $1"; }
fail() { echo -e "  ${RED}✗${NC} $1"; }

echo "Checking test dependencies..."
echo ""

# --- Python + debugpy ---
echo "Python:"
if command -v python3 &>/dev/null; then
    ok "python3 $(python3 --version 2>&1 | awk '{print $2}')"
else
    fail "python3 not found"
fi

if python3 -m debugpy --version &>/dev/null 2>&1; then
    ok "debugpy installed"
else
    warn "debugpy not found — installing..."
    pip3 install --user debugpy
    ok "debugpy installed"
fi

echo ""

# --- Node.js ---
echo "Node.js:"
if command -v node &>/dev/null; then
    ok "node $(node --version)"
else
    fail "node not found (needed for node adapter tests)"
fi

echo ""

# --- Go + Delve ---
echo "Go:"
if command -v go &>/dev/null; then
    ok "go $(go version | awk '{print $3}')"
    if command -v dlv &>/dev/null; then
        ok "dlv (delve) installed"
    else
        warn "dlv not found — installing..."
        go install github.com/go-delve/delve/cmd/dlv@latest
        ok "dlv installed"
    fi
else
    warn "go not found (needed for go adapter tests — skipping dlv)"
fi

echo ""
echo "Done. Missing tools will cause their adapter tests to be skipped."
