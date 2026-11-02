#!/usr/bin/env bash
# Runs the scripted end-to-end scenario. On a headless Linux box it uses
# Xvfb + Mesa lavapipe (software Vulkan); on macOS / a desktop it just opens
# a window. Usage: ./run_autotest.sh [screenshot_dir]
set -euo pipefail
cd "$(dirname "$0")"
SHOTS="${1:-target/autotest_shots}"
BIN=target/debug/aetherwright
cargo build
if [[ "$(uname)" == "Linux" && -z "${DISPLAY:-}" ]]; then
  export VK_ICD_FILENAMES="${VK_ICD_FILENAMES:-/usr/share/vulkan/icd.d/lvp_icd.json}"
  exec xvfb-run -a -s "-screen 0 1600x900x24" "$BIN" --autotest --screenshots "$SHOTS"
else
  exec "$BIN" --autotest --screenshots "$SHOTS"
fi
