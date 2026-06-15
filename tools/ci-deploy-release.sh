#!/usr/bin/env bash
#
# ci-deploy-release.sh -- publish pre-built, pre-signed desktop bundles to the VPS.
#
# This is the DEPLOY half of the release pipeline. The CI build matrix has
# already produced and SIGNED the Tauri v2 updater bundles (Tauri emits the
# .sig files because bundle.createUpdaterArtifacts is true) and uploaded them as
# workflow artifacts. This script downloads nothing and signs nothing -- it
# consumes the artifacts and pushes them to the VPS that nginx serves at:
#
#     https://$UPDATE_HOST/idc/<target>/x86_64/latest.json
#
# It is driven off the .SIG file, not the bundle name. For each platform it
# finds exactly one *.sig under the artifact dir; the file it signs is the same
# path without ".sig" (AppImage: foo.AppImage(.sig); NSIS updater: foo-setup
# .nsis.zip(.sig)). That artifact is what the manifest URL points at -- so the
# url always references the exact bytes the signature covers. (The plain
# -setup.exe used for first-time install is a separate artifact and is
# intentionally not what the updater downloads.)
#
# Per platform:
#   1. find exactly one .sig; derive bundle = ${sig%.sig}; assert bundle exists,
#   2. read the .sig CONTENTS verbatim (minisign sigs are TWO lines: a comment
#      line + the base64 line -- both must be preserved) and JSON-encode via jq,
#   3. write latest.json (schema below) with jq so every value is escaped,
#   4. rsync the BUNDLE first, then latest.json LAST, into the platform's own
#      remote dir -- a client polling mid-deploy never sees a manifest pointing
#      at a binary that is not on the server yet.
#
# Zero-downtime / rollback:
#   * rsync runs WITHOUT --delete, so old bundles stay for in-flight downloads
#     and rollback. The OLD latest.json keeps pointing at the OLD (still-present)
#     binary until the NEW binary is fully uploaded; overwriting latest.json is
#     the last step that flips the version live.
#   * Per-platform dirs mean a partial matrix (e.g. only Linux built) deploys
#     what exists and leaves the other platform's files untouched.
#
# latest.json schema (Tauri v2 static updater):
#   { "version": "X.Y.Z", "notes": "...", "pub_date": "ISO8601",
#     "platforms": { "<linux-x86_64|windows-x86_64>": {
#         "signature": "<verbatim contents of the .sig>",
#         "url": "https://<host>/idc/<target>/x86_64/<bundlefile>" } } }
#
# Required env (GitHub Actions secrets + the tag):
#   UPDATE_HOST       releases domain, NO scheme       e.g. releases.example.com
#   DEPLOY_SSH_USER   ssh user on the VPS              e.g. idcdeploy
#   DEPLOY_SSH_HOST   VPS ip/hostname
#   DEPLOY_DOCROOT    nginx docroot for /idc           e.g. /var/www/idc-updates
#   VERSION           release version, vX.Y.Z or X.Y.Z (normalized to X.Y.Z)
# Optional env:
#   ARTIFACTS_DIR     dir with linux/ and windows/ subdirs  (default: artifacts)
#   RELEASE_NOTES     manifest "notes"   (default: "IDC System <version>")
#   SSH_PORT          ssh port           (default: 22)
#
# Auth: ssh-agent is assumed loaded (workflow uses webfactory/ssh-agent) and
# ~/.ssh/known_hosts is assumed pinned by the workflow. No key material is read,
# echoed, or logged here.

set -euo pipefail

log()  { printf 'ci-deploy: %s\n' "$1"; }
warn() { printf 'ci-deploy: WARNING: %s\n' "$1" >&2; }
err()  { printf 'ci-deploy: ERROR: %s\n' "$1" >&2; exit 1; }

command -v jq >/dev/null    || err "jq is required (apt-get install jq)"
command -v rsync >/dev/null || err "rsync is required"

UPDATE_HOST="${UPDATE_HOST:-}"
DEPLOY_SSH_USER="${DEPLOY_SSH_USER:-}"
DEPLOY_SSH_HOST="${DEPLOY_SSH_HOST:-}"
DEPLOY_DOCROOT="${DEPLOY_DOCROOT:-}"
VERSION="${VERSION:-}"
ARTIFACTS_DIR="${ARTIFACTS_DIR:-artifacts}"
SSH_PORT="${SSH_PORT:-22}"

[ -n "$UPDATE_HOST" ]     || err "UPDATE_HOST is required (releases domain, no scheme)"
[ -n "$DEPLOY_SSH_USER" ] || err "DEPLOY_SSH_USER is required"
[ -n "$DEPLOY_SSH_HOST" ] || err "DEPLOY_SSH_HOST is required"
[ -n "$DEPLOY_DOCROOT" ]  || err "DEPLOY_DOCROOT is required (e.g. /var/www/idc-updates)"
[ -n "$VERSION" ]         || err "VERSION is required (vX.Y.Z or X.Y.Z)"
[ -d "$ARTIFACTS_DIR" ]   || err "artifacts dir not found: $ARTIFACTS_DIR"

case "$UPDATE_HOST" in
  *://*) err "UPDATE_HOST must NOT include a scheme (got '$UPDATE_HOST')" ;;
esac

VERSION="${VERSION#v}"
printf '%s' "$VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' \
  || err "VERSION must be X.Y.Z (optionally v-prefixed); got '$VERSION'"

RELEASE_NOTES="${RELEASE_NOTES:-IDC System ${VERSION}}"
ARCH="x86_64"
SSH_TARGET="${DEPLOY_SSH_USER}@${DEPLOY_SSH_HOST}"

# Non-interactive ssh. StrictHostKeyChecking=yes requires the workflow to have
# pinned the host key into ~/.ssh/known_hosts first (it does); a mismatch then
# fails closed rather than trusting an unknown host.
SSH_RSH="ssh -p ${SSH_PORT} -o BatchMode=yes -o StrictHostKeyChecking=yes"

PUB_DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

# Each row: <artifact-subdir>:<target>:<platform-key>
PLATFORMS=(
  "linux:linux:linux-x86_64"
  "windows:windows:windows-x86_64"
)

deployed_count=0

# find_one_sig <dir> -- echo the single .sig path. rc: 1 = zero, 2 = many.
find_one_sig () {
  local dir="$1" matches=() f
  while IFS= read -r -d '' f; do matches+=("$f"); done < <(
    find "$dir" -type f -name '*.sig' -print0 | sort -z
  )
  if [ "${#matches[@]}" -eq 0 ]; then return 1; fi
  if [ "${#matches[@]}" -gt 1 ]; then
    printf 'candidates:\n' >&2; printf '  %s\n' "${matches[@]}" >&2; return 2
  fi
  printf '%s' "${matches[0]}"
}

for row in "${PLATFORMS[@]}"; do
  IFS=":" read -r subdir target platform_key <<<"$row"
  pdir="${ARTIFACTS_DIR}/${subdir}"

  if [ ! -d "$pdir" ]; then
    warn "no artifacts for ${platform_key} (missing ${pdir}); skipping -- existing ${target} files on the server are left untouched"
    continue
  fi

  # Drive off the .sig: the bundle it signs IS the updater artifact whose URL
  # goes in the manifest. This sidesteps guessing the bundle extension (AppImage
  # vs -setup.nsis.zip) and guarantees signature and URL refer to the same bytes.
  sig=""; rc=0
  sig="$(find_one_sig "$pdir")" || rc=$?
  case "$rc" in
    1) err "no .sig found in ${pdir} for ${platform_key} -- signing did not run (is createUpdaterArtifacts enabled and the signing key set in the build job?)" ;;
    2) err "multiple .sig files in ${pdir} for ${platform_key} -- expected exactly one updater artifact, refusing to guess" ;;
  esac

  bundle="${sig%.sig}"
  [ -f "$bundle" ] || err "signature ${sig} has no sibling bundle ${bundle} for ${platform_key}"
  [ -s "$sig" ]    || err "signature ${sig} is empty for ${platform_key}"

  bundle_name="$(basename "$bundle")"
  download_url="https://${UPDATE_HOST}/idc/${target}/${ARCH}/${bundle_name}"

  # jq builds the manifest: --rawfile reads the .sig verbatim (both lines, with
  # the embedded newline) and JSON-encodes it; --arg escapes notes/version/url.
  manifest="${STAGE}/${platform_key}.latest.json"
  jq -n \
    --arg version "$VERSION" \
    --arg notes "$RELEASE_NOTES" \
    --arg pub_date "$PUB_DATE" \
    --arg platform_key "$platform_key" \
    --rawfile signature "$sig" \
    --arg url "$download_url" \
    '{
      version: $version,
      notes: $notes,
      pub_date: $pub_date,
      platforms: { ($platform_key): { signature: $signature, url: $url } }
    }' > "$manifest"

  # The VPS forces `rrsync -munge -wo $DEPLOY_DOCROOT` as the SSH command, which
  # (1) rejects any non-rsync command -- so NO `ssh ... mkdir`: rrsync would
  #     die("SSH_ORIGINAL_COMMAND does not run rsync"); rsync creates the dest
  #     dirs itself on the receiving side, and
  # (2) chdir's into $DEPLOY_DOCROOT and anchors every path there, rejecting
  #     absolute paths and "..". So the rsync destination MUST be RELATIVE to the
  #     docroot (an absolute path would get the docroot prepended -> doubled).
  remote_dir="idc/${target}/${ARCH}"
  log "deploying ${platform_key}: ${bundle_name} -> ${SSH_TARGET}:${DEPLOY_DOCROOT}/${remote_dir}"

  # STEP 1: binary first. No --delete (keep old bundles); --chmod=F644 so nginx
  # (www-data) can read it regardless of the deploy user's umask. rsync creates
  # the intermediate dirs on the server because the dest path includes them.
  rsync -a --chmod=F644 --mkpath -e "$SSH_RSH" \
    "$bundle" "${SSH_TARGET}:${remote_dir}/${bundle_name}"

  # STEP 2: manifest last -- the flip that makes the new version live.
  rsync -a --chmod=F644 --mkpath -e "$SSH_RSH" \
    "$manifest" "${SSH_TARGET}:${remote_dir}/latest.json"

  log "published ${platform_key} ${VERSION} -> https://${UPDATE_HOST}/idc/${target}/${ARCH}/latest.json"
  deployed_count=$((deployed_count + 1))
done

[ "$deployed_count" -gt 0 ] \
  || err "no platforms deployed -- ${ARTIFACTS_DIR} contained neither linux/ nor windows/ artifacts"

log "done: ${deployed_count} platform(s) published for ${VERSION}"
