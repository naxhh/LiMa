#!/usr/bin/env bash
set -euo pipefail
API_BASE="${API_BASE:-http://localhost:6767}"
FILES=(
  "$HOME/Descargas/ColumnSupport.3mf"
  "$HOME/Descargas/disc.stl"
  "$HOME/Descargas/pexels-francesco-ungaro-1526713.jpg"
)


curl_args=(-sS -X POST "$API_BASE/bundles")
for f in "${FILES[@]}"; do
    [[ -r "$f" ]] || { echo "Missing/unreadable file: $f" >&2; exit 1; }
    curl_args+=(-F "files[]=@$f")
done

curl "${curl_args[@]}" | jq
