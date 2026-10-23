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
# does not. A new dirty file, a shifted hunk, or a changed hunk body fails.
#
# This script does not apt-get install. The blob's apt list includes
# libxcb1-dev and libfreetype6-dev. Those packages are not installed here.
# This script runs build, test, clippy, and the fmt gate even when an
# earlier step fails, then exits with ci_fail. GitHub Actions stops the
# job at the first failing step.
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
# Exit 0 from `cargo test` is not a pass. Require one full anchored
# cargo summary line whose passed count is greater than zero. A runner
# that skips the harness (for example runner = "true") exits 0 with no
# such line, and a decoy substring after a zero-passed summary does not
# count.
if ! "${SCRIPT_DIR}/check-test-result.sh" "${LOG_DIR}/test.log"; then
  fail=1
fi
run_one clippy cargo clippy --all-targets -- -D warnings || fail=1

echo "===== fmt: scripts/fmt-gate.sh ====="
fmt_log="${LOG_DIR}/fmt-gate.log"
set +e
"${SCRIPT_DIR}/fmt-gate.sh" >"${fmt_log}" 2>&1
fmt_status=$?
set -e
cat "${fmt_log}"
echo "===== fmt exit=${fmt_status} ====="
if [[ "${fmt_status}" -ne 0 ]]; then
  fail=1
elif ! grep -Fxq 'fmt_gate=pass' "${fmt_log}"; then
  echo "fmt_gate_pass=missing"
  fail=1
fi

echo "ci_fail=${fail}"
exit "${fail}"
