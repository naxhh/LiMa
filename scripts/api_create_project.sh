#!/usr/bin/env bash
set -euo pipefail

API_BASE="${API_BASE:-http://localhost:6767}"
FILES=(
  "$HOME/Descargas/ColumnSupport.3mf"
  "$HOME/Descargas/disc.stl"
  "$HOME/Descargas/pexels-francesco-ungaro-1526713.jpg"
)

date_str="$(date -u +%Y%m%dT%H%M%SZ)"
project_name="Test upload - ${date_str}"


# bundle_json=$(./api_create_bundle.sh)

project_json="$(curl -sS -X POST "$API_BASE/projects" \
  -H 'content-type: application/json' \
  -d "{\"name\":\"$project_name\",\"description\":\"this is a project created during dev time\",\"tags\":[\"testing\",\"API\"]}")"

project_id="$(jq -r '.id // empty' <<<"$project_json")"
if [[ -z "$project_id" ]]; then
  echo "Failed to create project. Response:" >&2
  echo "$project_json" | jq -C . >&2 || echo "$project_json" >&2
  exit 1
fi

echo "Created project: $project_id ($project_name)"

