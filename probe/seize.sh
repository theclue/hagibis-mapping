#!/usr/bin/env bash
# seize.sh — Launch the HID seize tool (requires root for device seizing)
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec sudo "${SCRIPT_DIR}/.venv/bin/python" "${SCRIPT_DIR}/seize.py"
