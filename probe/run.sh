#!/usr/bin/env bash
# run.sh — Activate venv and launch the HID monitor
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/.venv/bin/activate"
exec python "${SCRIPT_DIR}/monitor.py"
