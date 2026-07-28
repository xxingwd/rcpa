#!/usr/bin/env bash
set -euo pipefail

exec cargo run -- \
  --token "${RCPA_ADMIN_TOKEN:-local-admin-token}" \
  --data-dir "${RCPA_DATA_DIR:-data}" \
  --port "${RCPA_PORT:-15000}" \
  --log-level "${RCPA_LOG_LEVEL:-info}"
