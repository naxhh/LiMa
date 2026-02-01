#!/usr/bin/env bash
set -euo pipefail

API_BASE="${API_BASE:-http://localhost:6767}"
BUNDLES_DIR="${BUNDLES_DIR:-data/state/bundles}"

delete_bundle() {
  local bundle_id="$1"

  response="$(curl -sS -w '\n%{http_code}' \
    -X DELETE "$API_BASE/bundles/$bundle_id")"

  body="$(sed '$d' <<<"$response")"
  status="$(tail -n1 <<<"$response")"

  if [[ -n "$body" ]]; then
    echo "$body" | jq -C . || echo "$body"
  fi

  if [[ "$status" != "200" ]]; then
    echo "Failed to delete bundle $bundle_id" >&2
    echo $body >&2
    return 1
  fi

  echo "✅ Bundle $bundle_id deleted"
}

# ──────────────────────────────────────────────────────────────
# Mode 1: bundle id provided
# ──────────────────────────────────────────────────────────────
if [[ $# -eq 1 ]]; then
  delete_bundle "$1"
  exit 0
fi

# ──────────────────────────────────────────────────────────────
# Mode 2: no args → iterate bundle folders
# ──────────────────────────────────────────────────────────────
if [[ $# -gt 1 ]]; then
  echo "Usage: $0 [bundle_id]" >&2
  exit 1
fi

if [[ ! -d "$BUNDLES_DIR" ]]; then
  echo "Bundles directory does not exist: $BUNDLES_DIR" >&2
  exit 0
fi

echo "No bundle ID provided. Cleaning all bundles in $BUNDLES_DIR"

shopt -s nullglob
bundle_dirs=("$BUNDLES_DIR"/*)
shopt -u nullglob

if [[ ${#bundle_dirs[@]} -eq 0 ]]; then
  echo "No bundles found."
  exit 0
fi

errors=0

for dir in "${bundle_dirs[@]}"; do
  bundle_id="$(basename "$dir")"

  if [[ ! -d "$dir" ]]; then
    continue
  fi

  if ! delete_bundle "$bundle_id"; then
    errors=$((errors + 1))
  fi
done

if [[ $errors -gt 0 ]]; then
  echo "Finished with $errors error(s)" >&2
  exit 1
fi

echo "All bundles deleted successfully."
