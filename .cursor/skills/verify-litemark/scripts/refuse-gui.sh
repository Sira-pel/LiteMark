#!/usr/bin/env bash
# Refuse to launch the LiteMark window. Exit 2.
#
# The two reasons are exact lines. Do not treat exit 2 as a verified UI path.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

echo "REFUSED bin=${LITEMARK_BIN}"
echo "reason=notes_dir_unisolated path=dirs::document_dir()/Notes symbol=get_or_create_notes_dir file=src/services/fs_service.rs"
echo "reason=vulkan_icd_missing path=/usr/share/vulkan/icd.d"
echo "command=not-run target/debug/litemark"
exit 2
