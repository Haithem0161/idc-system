#!/usr/bin/env bash
# Phase-09 pre-ship guardrails. Enforces invariants the test suites alone
# can't catch by parsing the source tree. CI runs this in addition to
# `pre-push-check.sh`.

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

# Stale phase-04 forward-references in Rust source. Removed under NIT-2;
# this guards against reintroduction.
step "Guardrail: no 'phase-04' forward-ref in src-tauri/src/**/*.rs"
if grep -rn -E "phase-04 hardens|TODO.*phase-04" src-tauri/src/ 2>/dev/null; then
  fail "src-tauri/src contains stale phase-04 references"
fi
ok "no phase-04 forward-references"

# Banner eprintln! in lib.rs embedded-mode block. Removed under NIT-3.
# Excludes src/bin/: standalone CLI tools (e.g. the seed_weekly seeder) write
# operator-facing progress to stderr via eprintln! by design -- that is the
# correct CLI idiom, not the in-runtime "use tracing" rule this guard enforces
# for the app/library code that runs inside the Tauri webview.
step "Guardrail: no eprintln! / println! in src-tauri/src (excluding src/bin/ CLI tools)"
if grep -rn -E "^[^/]*\\b(eprintln|println)!" --exclude-dir=bin src-tauri/src/ 2>/dev/null; then
  fail "src-tauri/src contains eprintln!/println! (use tracing instead)"
fi
ok "no eprintln!/println! in src-tauri/src (excluding src/bin/)"

# 'dev-only-secret' literal in JWT plugin. Removed under BLOCKER-2; the
# CI grep guardrail enforces it never reappears.
step "Guardrail: no 'dev-only-secret' literal in sync-server/src"
if grep -rn "dev-only-secret" sync-server/src/ 2>/dev/null; then
  fail "sync-server/src contains 'dev-only-secret' literal (BLOCKER-2 regression)"
fi
ok "no 'dev-only-secret' fallback"

# Defunct SYNC_STORE env-var hint. Replaced by DATABASE_URL probing per
# BLOCKER-3 wiring.
step "Guardrail: no SYNC_STORE env-var comment in sync-server/src"
if grep -rn "SYNC_STORE" sync-server/src/ 2>/dev/null; then
  fail "sync-server/src references deprecated SYNC_STORE env var"
fi
ok "no SYNC_STORE references"

# .env files MUST NEVER be committed. .env.template is the only allowed
# env file in version control.
step "Guardrail: sync-server/.env is NOT tracked by git"
if [[ -n "$(git ls-files sync-server/.env)" ]]; then
  fail "sync-server/.env is tracked by git -- remove it from the index"
fi
ok "sync-server/.env not in version control"

step "Guardrail: root .env is NOT tracked by git"
if [[ -n "$(git ls-files .env)" ]]; then
  fail ".env is tracked by git -- remove it from the index"
fi
ok "root .env not in version control"

echo
echo -e "${GREEN}All pre-ship guardrails passed.${NC}"
