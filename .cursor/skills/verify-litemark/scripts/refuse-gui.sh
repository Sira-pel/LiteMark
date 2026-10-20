#!/usr/bin/env bash
# Refuse to launch the LiteMark window. Exit 2.
#
# Feature journeys that need the desktop window call this and stop.
# Do not treat exit 2 as a verified UI path.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

echo "REFUSED bin=${LITEMARK_BIN}"
echo "unmet_precondition=notes directory is dirs::document_dir()/Notes with no override (src/services/fs_service.rs get_or_create_notes_dir); launch writes that directory's Welcome.md"
echo "unmet_precondition=screenshot tools import, scrot, and grim are not installed; imagemagick is not an allowed install"
echo "unmet_precondition=no Vulkan ICD in /usr/share/vulkan/icd.d; mesa-vulkan-drivers is allowed only after an EGL failure, and this kit does not take the screenshot path"
echo "command=not-run target/debug/litemark"
exit 2
