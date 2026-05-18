#!/usr/bin/env bash
# Pre-push validation script -- mirrors what CI runs.
# Referenced by .claude/rules/dev-workflow.md and CLAUDE.md as MANDATORY before every push.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

CYAN='\033[0;36m'
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

step() { echo -e "${CYAN}==>${NC} $*"; }
ok()   { echo -e "${GREEN}OK${NC}: $*"; }
fail() { echo -e "${RED}FAIL${NC}: $*"; exit 1; }

# --- Frontend -----------------------------------------------------------------
step "Frontend: pnpm lint"
pnpm lint || fail "pnpm lint"
ok "pnpm lint"

step "Frontend: pnpm build"
pnpm build || fail "pnpm build"
ok "pnpm build"

# --- Tauri / Rust -------------------------------------------------------------
step "Tauri: cargo fmt --check"
( cd src-tauri && cargo fmt --check ) || fail "cargo fmt --check"
ok "cargo fmt"

step "Tauri: cargo clippy --all-targets -- -D warnings"
( cd src-tauri && cargo clippy --all-targets -- -D warnings ) || fail "cargo clippy"
ok "cargo clippy"

step "Tauri: cargo test"
( cd src-tauri && cargo test ) || fail "cargo test"
ok "cargo test"

# --- Sync Server (only when present) -----------------------------------------
if [ -d sync-server ] && [ -f sync-server/package.json ]; then
  step "Sync server: pnpm test"
  ( cd sync-server && pnpm test ) || fail "sync-server pnpm test"
  ok "sync-server pnpm test"
else
  step "Sync server: skipped (not present)"
fi

# --- Phase-09 pre-ship guardrails --------------------------------------------
step "Pre-ship guardrails (phase-09 SHIP-CONCERN regression checks)"
./tools/preship-guardrails.sh || fail "preship-guardrails.sh"
ok "preship guardrails"

echo
echo -e "${GREEN}All pre-push checks passed.${NC}"
