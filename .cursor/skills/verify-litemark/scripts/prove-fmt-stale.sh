#!/usr/bin/env bash
# rustfmt src/theme.rs drops the baseline hunk at src/theme.rs:22.
# Unmatched baseline tokens print fmt_baseline_stale=<n> and the gate fails.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

target="${ROOT}/src/theme.rs"
backup="${VERIFY_RUN_DIR}/theme.rs.stale.bak"
log="${VERIFY_EVIDENCE_DIR}/fmt-stale.log"
cp "${target}" "${backup}"
restore() {
  cp "${backup}" "${target}"
}
trap restore EXIT

rustfmt --edition 2021 --config-path "${KIT_DIR}/rustfmt.toml" "${target}"

set +e
"${SCRIPT_DIR}/fmt-gate.sh" >"${log}" 2>&1
status=$?
set -e

restore
trap - EXIT
if ! cmp -s "${target}" "${backup}"; then
  echo "restore failed for src/theme.rs" >&2
  exit 1
fi
echo "restored=src/theme.rs"

if [[ "${status}" -eq 0 ]]; then
  echo "fmt-gate passed after rustfmt src/theme.rs" >&2
  exit 1
fi
if ! grep -Fxq "fmt_baseline_stale=1" "${log}"; then
  echo "missing fmt_baseline_stale=1" >&2
  grep -E '^fmt_' "${log}" >&2 || true
  exit 1
fi
if ! grep -Fxq "fmt_gate=fail" "${log}"; then
  echo "missing fmt_gate=fail" >&2
  exit 1
fi
if ! grep -Fxq "fmt_observed_hunks=200" "${log}"; then
  echo "missing fmt_observed_hunks=200" >&2
  grep -E '^fmt_' "${log}" >&2 || true
  exit 1
fi

echo "RED fmt result=FAIL_AS_REQUIRED gate_exit=${status}"
echo "fmt_baseline_stale=1"
echo "fmt_gate=fail"
grep -E '^(fmt_baseline_stale=|fmt_observed_hunks=|fmt_gate=|fmt_baseline_hunks=|fmt_cargo_status=)' "${log}"
echo "proof=L-3 rc=${status} line=fmt_baseline_stale=1"
echo "log=${log}"
