#!/usr/bin/env bash
set -euo pipefail


API_BASE="${API_BASE:-http://localhost:6767}"

date_str="$(date -u +%Y%m%dT%H%M%SZ)"
project_name="Example upload - ${date_str}"

project_json="$(curl -sS -X POST "$API_BASE/projects" \
  -H 'content-type: application/json' \
  -d "{\"name\":\"$project_name\",\"description\":\"\",\"tags\":[\"testing\",\"API\"]}")"

project_id="$(jq -r '.id // empty' <<<"$project_json")"
if [[ -z "$project_id" ]]; then
  echo "Failed to create project. Response:" >&2
  echo "$project_json" | jq -C . >&2 || echo "$project_json" >&2
  exit 1
fi

echo "Created project: $project_id ($project_name)"

# Build multipart args
curl_args=(-v -X POST "$API_BASE/projects/$project_id/assets")
curl_args+=(-F "main_image=pexels-francesco-ungaro-1526713.jpg")

FILES=(
  "$HOME/Descargas/ColumnSupport.3mf"
  "$HOME/Descargas/disc.stl"
  "$HOME/Descargas/pexels-francesco-ungaro-1526713.jpg"
)

for f in "${FILES[@]}"; do
    [[ -r "$f" ]] || { echo "Missing/unreadable file: $f" >&2; exit 1; }
    curl_args+=(-F "files[]=@$f")
done

curl "${curl_args[@]}" | jq
echo "Files uploaded to project $project_id"
