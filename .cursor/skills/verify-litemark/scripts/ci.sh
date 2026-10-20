#!/usr/bin/env bash
# CI contract until main has .github/workflows.
#
# Provenance: branch cursor/linux-packaging-readme-4544,
# blob a24e5dd3af5ff2f6b0a7bfdad3530f4e580920db (.github/workflows/ci.yml).
# main does not contain that file.
#
# Contract copied from that blob, with the fmt step replaced:
#   cargo build
#   cargo test
#   cargo clippy --all-targets -- -D warnings
#   scripts/fmt-gate.sh
# The blob's fmt step skips when `cargo fmt --check` is dirty. This script
# does not. A new dirty file or a new hunk fails the script.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

LOG_DIR="${VERIFY_EVIDENCE_DIR}/ci"
mkdir -p "${LOG_DIR}"

run_one() {
  local name="$1"
  shift
  local log="${LOG_DIR}/${name}.log"
  echo "===== ${name}: $* ====="
  set +e
  "$@" >"${log}" 2>&1
  local status=$?
  set -e
  cat "${log}"
  echo "===== ${name} exit=${status} log=${log} ====="
  return "${status}"
}

fail=0
run_one build cargo build || fail=1
run_one test cargo test || fail=1
run_one clippy cargo clippy --all-targets -- -D warnings || fail=1

echo "===== fmt: scripts/fmt-gate.sh ====="
set +e
"${SCRIPT_DIR}/fmt-gate.sh"
fmt_status=$?
set -e
echo "===== fmt exit=${fmt_status} ====="
if [[ "${fmt_status}" -ne 0 ]]; then
  fail=1
fi

echo "ci_fail=${fail}"
exit "${fail}"
