#!/usr/bin/env bash
# bootstrap.sh — Setup script for USB Hub HID Monitor (CGEvent Tap)
#
# Creates a Python virtual environment, installs dependencies,
# and verifies that the environment is ready.
#
# Usage: ./bootstrap.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENV_DIR="${SCRIPT_DIR}/.venv"

# ── Colors ──────────────────────────────────────────────────────────────────

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

info()  { printf "${GREEN}[INFO]${NC}  %s\n" "$*"; }
warn()  { printf "${YELLOW}[WARN]${NC}  %s\n" "$*"; }
error() { printf "${RED}[ERROR]${NC} %s\n" "$*"; }
header(){ printf "\n${BOLD}━━━ %s ━━━${NC}\n" "$*"; }

# ── Step 1: Check prerequisites ─────────────────────────────────────────────

header "Checking prerequisites"

# Python 3
if command -v python3 &>/dev/null; then
    PYTHON="$(command -v python3)"
    info "Python 3: ${PYTHON} ($(${PYTHON} --version 2>&1))"
else
    error "python3 not found. Install Python 3.9+ first."
    exit 1
fi

# ── Step 2: Create virtual environment ──────────────────────────────────────

header "Setting up Python virtual environment"

if [ -d "${VENV_DIR}" ]; then
    info "Virtualenv already exists at ${VENV_DIR}"
else
    info "Creating virtualenv..."
    ${PYTHON} -m venv "${VENV_DIR}"
    info "Virtualenv created."
fi

# Activate
# shellcheck disable=SC1091
source "${VENV_DIR}/bin/activate"
info "Activated virtualenv (Python: $(python --version 2>&1))"

# ── Step 3: Upgrade pip ─────────────────────────────────────────────────────

info "Upgrading pip..."
pip install --upgrade pip --quiet 2>&1 | tail -1 || true

# ── Step 4: Install Python dependencies ─────────────────────────────────────

header "Installing Python dependencies"

pip install -r "${SCRIPT_DIR}/requirements.txt" 2>&1

info "Python packages installed:"
pip list 2>/dev/null | grep -iE "pyobjc|hidapi" || true

# ── Step 5: Verify pyobjc/Quartz works ─────────────────────────────────────

header "Verifying pyobjc Quartz"

python -c "
import Quartz
print('  Quartz framework: OK')

# Check if Accessibility permission is granted
tap = Quartz.CGEventTapCreate(
    Quartz.kCGSessionEventTap,
    Quartz.kCGHeadInsertEventTap,
    Quartz.kCGEventTapOptionDefault,
    (1 << Quartz.kCGEventKeyDown),
    lambda p,t,e,r: e,
    None
)
if tap is not None:
    print('  Accessibility permission: GRANTED')
    Quartz.CFMachPortInvalidate(tap)
else:
    print('  Accessibility permission: MISSING')
" 2>&1

# ── Step 6: macOS permissions reminder ──────────────────────────────────────

if [[ "$(uname)" == "Darwin" ]]; then
    header "macOS Accessibility permission"

    echo ""
    echo "  This tool uses CGEvent Tap, which requires ACCESSIBILITY permission."
    echo "  (NOT Input Monitoring — that was for hidapi)."
    echo ""
    echo "  If the monitor fails to start:"
    echo "    1. System Settings → Privacy & Security → Accessibility"
    echo "    2. Add your terminal app (Terminal, iTerm, VS Code, etc.)"
    echo "    3. Toggle it ON and restart the terminal"
    echo ""

    # Check if device is visible in USB tree (informational only)
    if system_profiler SPUSBHostDataType 2>/dev/null | grep -q "0x05ac"; then
        info "Hub device visible in USB tree"
    else
        warn "Hub NOT found in USB tree. Is it connected?"
    fi
fi

# ── Done ─────────────────────────────────────────────────────────────────────

header "Setup complete"

echo ""
echo "  To run the monitor:"
echo "    cd ${SCRIPT_DIR}"
echo "    source .venv/bin/activate"
echo "    python monitor.py"
echo ""
echo "  Or simply:"
echo "    ${SCRIPT_DIR}/run.sh"
echo ""

# Make run.sh executable
chmod +x "${SCRIPT_DIR}/run.sh" 2>/dev/null || true
