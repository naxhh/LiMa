#!/usr/bin/env bash
set -euo pipefail

rm -Rf ./data/
mkdir -p ./data/state/
touch ./data/state/lima.db