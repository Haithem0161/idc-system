#!/usr/bin/env bash
#
# release-update.sh -- build, sign, and publish a desktop self-update.
#
# What it does, in order:
#   1. Builds the signed Tauri bundle for the platform you run it ON
#      (Linux -> AppImage, Windows -> NSIS .exe). Cross-building is NOT
#      supported -- run this once per target OS on a machine of that OS.
#   2. Reads the version from src-tauri/tauri.conf.json.
#   3. Writes a per-platform latest.json manifest (signature + download URL).
#   4. Uploads the bundle + manifest to the VPS over scp, into the docroot
#      nginx serves at https://$UPDATE_HOST/idc/<target>/<arch>/.
#
# The app's tauri.conf.json endpoint is:
#   https://$UPDATE_HOST/idc/{{target}}/{{arch}}/latest.json
# The plugin substitutes {{target}}/{{arch}}, GETs that static file, compares
# its `version` to the running version, and (if newer) downloads + verifies the
# signature against the baked-in pubkey before installing.
#
# Each run only touches ITS OWN platform's directory, so a Linux run and a
# Windows run coexist without clobbering each other.
#
# Required env (set these or edit the defaults below):
#   UPDATE_HOST                 e.g. releases.example.com  (NO scheme)
#   DEPLOY_SSH                  e.g. deploy@1.2.3.4         (ssh target)
#   DEPLOY_DOCROOT              e.g. /var/www/idc-updates  (nginx root for /idc)
#   TAURI_SIGNING_PRIVATE_KEY   contents OR use _PATH below
#   TAURI_SIGNING_PRIVATE_KEY_PASSWORD   (empty if the key has no password)
#
# Optional:
#   TAURI_SIGNING_PRIVATE_KEY_PATH   path to the key file (default ~/.idc/updater.key)
#
# Usage:
#   UPDATE_HOST=releases.example.com DEPLOY_SSH=deploy@host \
#   DEPLOY_DOCROOT=/var/www/idc-updates ./tools/release-update.sh

set -euo pipefail

# --- config (env overrides win) ----------------------------------------------
UPDATE_HOST="${UPDATE_HOST:-}"
DEPLOY_SSH="${DEPLOY_SSH:-}"
DEPLOY_DOCROOT="${DEPLOY_DOCROOT:-/var/www/idc-updates}"
KEY_PATH="${TAURI_SIGNING_PRIVATE_KEY_PATH:-$HOME/.idc/updater.key}"

err() { printf 'release-update: %s\n' "$1" >&2; exit 1; }

[ -n "$UPDATE_HOST" ] || err "UPDATE_HOST is required (e.g. releases.example.com, no scheme)"
[ -n "$DEPLOY_SSH" ]  || err "DEPLOY_SSH is required (e.g. deploy@1.2.3.4)"

# Tauri reads the key from the environment. Prefer an explicit string; otherwise
# load it from the key file so you don't have to export the secret by hand.
if [ -z "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
  [ -f "$KEY_PATH" ] || err "no signing key: set TAURI_SIGNING_PRIVATE_KEY or place the key at $KEY_PATH"
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$KEY_PATH")"
  export TAURI_SIGNING_PRIVATE_KEY
fi
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="$(grep -oP '"version":\s*"\K[^"]+' src-tauri/tauri.conf.json | head -1)"
[ -n "$VERSION" ] || err "could not read version from src-tauri/tauri.conf.json"

# --- detect platform ----------------------------------------------------------
UNAME="$(uname -s)"
ARCH="x86_64"   # the only arch this project targets today
case "$UNAME" in
  Linux*)   TARGET="linux";   PLATFORM_KEY="linux-x86_64";   EXT="AppImage" ;;
  MINGW*|MSYS*|CYGWIN*) TARGET="windows"; PLATFORM_KEY="windows-x86_64"; EXT="nsis.zip" ;;
  *) err "unsupported build OS: $UNAME (run on Linux for AppImage or Windows for the installer)" ;;
esac

printf 'release-update: building IDC %s for %s...\n' "$VERSION" "$PLATFORM_KEY"

# --- build (Tauri signs because the signing env is set) -----------------------
pnpm tauri build

# --- locate the signed bundle + its .sig --------------------------------------
BUNDLE_DIR="src-tauri/target/release/bundle"
BUNDLE="$(find "$BUNDLE_DIR" -type f -name "*.${EXT}" | head -1)"
[ -n "$BUNDLE" ] || err "no .${EXT} bundle found under $BUNDLE_DIR (did the build fail?)"
SIG_FILE="${BUNDLE}.sig"
[ -f "$SIG_FILE" ] || err "no signature next to $BUNDLE -- signing did not run (check TAURI_SIGNING_PRIVATE_KEY)"

BUNDLE_NAME="$(basename "$BUNDLE")"
SIGNATURE="$(cat "$SIG_FILE")"
DOWNLOAD_URL="https://${UPDATE_HOST}/idc/${TARGET}/${ARCH}/${BUNDLE_NAME}"

# --- write the manifest -------------------------------------------------------
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
PUB_DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

cat > "$STAGE/latest.json" <<JSON
{
  "version": "${VERSION}",
  "notes": "IDC System ${VERSION}",
  "pub_date": "${PUB_DATE}",
  "platforms": {
    "${PLATFORM_KEY}": {
      "signature": "${SIGNATURE}",
      "url": "${DOWNLOAD_URL}"
    }
  }
}
JSON

cp "$BUNDLE" "$STAGE/$BUNDLE_NAME"

# --- upload -------------------------------------------------------------------
REMOTE_DIR="${DEPLOY_DOCROOT}/idc/${TARGET}/${ARCH}"
printf 'release-update: uploading to %s:%s...\n' "$DEPLOY_SSH" "$REMOTE_DIR"
ssh "$DEPLOY_SSH" "mkdir -p '${REMOTE_DIR}'"
scp "$STAGE/$BUNDLE_NAME" "$STAGE/latest.json" "${DEPLOY_SSH}:${REMOTE_DIR}/"

printf 'release-update: published %s (%s)\n' "$VERSION" "$PLATFORM_KEY"
printf 'release-update: manifest -> https://%s/idc/%s/%s/latest.json\n' "$UPDATE_HOST" "$TARGET" "$ARCH"
