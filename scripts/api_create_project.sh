#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
API_BASE="${API_BASE:-http://localhost:6767/api}"
FILES=(
  "$HOME/Descargas/ColumnSupport.3mf"
  "$HOME/Descargas/disc.stl"
  "$HOME/Descargas/pexels-francesco-ungaro-1526713.jpg"
)

date_str="$(date -u +%Y%m%dT%H%M%S.%3NZ)"
project_name="Test upload - ${date_str}"


bundle_json="$("$SCRIPT_DIR/api_create_bundle.sh")"
bundle_id="$(jq -r '.id // empty' <<<"$bundle_json")"
if [[ -z "$bundle_id" ]]; then
  echo "Failed to create bundle. Response:" >&2
  echo "$bundle_json" >&2
  exit 1
fi

project_json="$(curl -sS -X POST "$API_BASE/projects" \
  -H 'content-type: application/json' \
  -d "{\"name\":\"$project_name\",\"description\":\"this is a project created during dev time\",\"tags\":[\"testing\",\"API\"]}")"

project_id="$(jq -r '.id // empty' <<<"$project_json")"
if [[ -z "$project_id" ]]; then
  echo "Failed to create project. Response:" >&2
  echo "$project_json" | jq -C . >&2 || echo "$project_json" >&2
  exit 1
fi

curl -sS -X POST "$API_BASE/projects/$project_id/import" \
  -H 'content-type: application/json' \
  -d "{\"bundle_id\":\"$bundle_id\"}"

echo "Created project: $project_id ($project_name)"

