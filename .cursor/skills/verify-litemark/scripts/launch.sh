#!/usr/bin/env bash
# Build the LiteMark binary. Does not start the GUI.
#
# The desktop process writes ~/Documents/Notes via dirs::document_dir()
# (src/services/fs_service.rs) and has no isolation flag. Screenshot tools
# are not installed. Verification drives the headless render dump instead.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

LOG="${VERIFY_RUN_DIR}/logs/cargo-build.log"
echo "cargo build (package litemark, no extra flags; .github/workflows is absent)"
set +e
cargo build >"${LOG}" 2>&1
status=$?
set -e
if [[ "${status}" -ne 0 ]]; then
  cat "${LOG}" >&2
  exit "${status}"
fi

if [[ ! -x "${LITEMARK_BIN}" ]]; then
  echo "cargo build exited 0 but ${LITEMARK_BIN} is missing" >&2
  exit 1
fi

printf '%s\n' "${LITEMARK_BIN}" >"${VERIFY_RUN_DIR}/bin.path"
echo "READY bin=${LITEMARK_BIN}"
echo "gui=not-started reason=notes-dir-not-isolated"
