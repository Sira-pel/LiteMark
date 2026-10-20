#!/usr/bin/env bash
# Commands CI runs.
#
# main has no .github directory (`git ls-tree -r HEAD --name-only`).
# The only workflow in this remote is
# origin/cursor/linux-packaging-readme-4544:.github/workflows/ci.yml.
# Its steps, copied here, are:
#   cargo build
#   cargo test
#   cargo clippy --all-targets -- -D warnings
#   cargo fmt --check, and if that fails the CI step prints a skip line
#   and still exits 0.
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

echo "===== fmt: cargo fmt --check ====="
fmt_log="${LOG_DIR}/fmt.log"
set +e
cargo fmt --check >"${fmt_log}" 2>&1
fmt_status=$?
set -e
cat "${fmt_log}"
if [[ "${fmt_status}" -eq 0 ]]; then
  echo "Tree is fmt-clean."
  echo "===== fmt exit=0 log=${fmt_log} ====="
else
  echo "Tree is not fmt-clean; skipping cargo fmt --check."
  echo "===== fmt raw_exit=${fmt_status} ci_step_exit=0 log=${fmt_log} ====="
fi

echo "ci_fail=${fail}"
exit "${fail}"
