#!/usr/bin/env bash
set -euo pipefail

API_BASE="${API_BASE:-http://localhost:6767/api}"

projects_json="$(curl -sS "$API_BASE/projects?limit=1")"

count="$(jq -r '.items | length' <<<"$projects_json")"
if [[ "$count" == "0" ]]; then
  echo "No projects found." >&2
  exit 1
fi

project_id="$(jq -r '.items[0].id // empty' <<<"$projects_json")"
project_name="$(jq -r '.items[0].name // empty' <<<"$projects_json")"


ts="$(date -u +%Y%m%dT%H%M%SZ)"
new_name="${project_name} [patched ${ts}]"

payload="$(jq -n --arg name "$new_name" --arg desc "Updated via script ${ts}" \
  '{name:$name, description:$desc}')"

echo $payload

curl -sS -i -X PATCH "$API_BASE/projects/$project_id" \
  -H 'content-type: application/json' \
  -d "$payload"
