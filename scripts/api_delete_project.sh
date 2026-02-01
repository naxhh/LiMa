#!/usr/bin/env bash
set -euo pipefail

API_BASE="${API_BASE:-http://localhost:6767}"

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <project_id>" >&2
  exit 1
fi

PROJECT_ID="$1"

response="$(curl -sS -w '\n%{http_code}' \
  -X DELETE "$API_BASE/projects/$PROJECT_ID")"

body="$(sed '$d' <<<"$response")"
status="$(tail -n1 <<<"$response")"

if [[ -n "$body" ]]; then
  echo "$body" | jq -C . || echo "$body"
fi

if [[ "$status" != "200" ]]; then
  echo "Delete failed" >&2
  echo $body >&2
  exit 1
fi

echo "Project deleted successfully."
