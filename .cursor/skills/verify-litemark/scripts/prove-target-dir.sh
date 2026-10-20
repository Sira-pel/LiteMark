#!/usr/bin/env bash
# launch.sh resolves the debug binary from cargo metadata, which follows
# CARGO_TARGET_DIR. A stale $ROOT/target/debug/litemark is not READY.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

stale="${ROOT}/target/debug/litemark"
target_dir="/tmp/else target"
ready_bin="${target_dir}/debug/litemark"
if [[ ! -x "${stale}" ]]; then
  echo "missing stale binary ${stale}" >&2
  exit 1
fi
stale_before="$(sha256sum "${stale}")"
log="${VERIFY_EVIDENCE_DIR}/target-dir.log"

set +e
LITEMARK_BIN="${stale}" \
  CARGO_TARGET_DIR="${target_dir}" \
  "${SCRIPT_DIR}/launch.sh" >"${log}" 2>&1
status=$?
set -e
if [[ "${status}" -ne 0 ]]; then
  echo "launch.sh failed" >&2
  cat "${log}" >&2
  exit 1
fi
ready_line="READY bin=${ready_bin}"
if ! grep -Fxq "${ready_line}" "${log}"; then
  echo "missing ${ready_line}" >&2
  grep '^READY ' "${log}" >&2 || true
  exit 1
fi
if grep -Fxq "READY bin=${stale}" "${log}"; then
  echo "READY used the stale workspace binary" >&2
  exit 1
fi
stale_after="$(sha256sum "${stale}")"
if [[ "${stale_before}" != "${stale_after}" ]]; then
  echo "workspace binary changed during CARGO_TARGET_DIR build" >&2
  exit 1
fi
if [[ ! -x "${ready_bin}" ]]; then
  echo "missing ${ready_bin}" >&2
  exit 1
fi
echo "${ready_line}"
echo "stale_unused=1"
echo "proof=M5 rc=${status} line=${ready_line}"
echo "log=${log}"
