#!/usr/bin/env bash
# Shift: 20 comment lines above the src/theme.rs hunk move its header from
# :22 to :42. The body text is the old hunk. The gate must exit 1 and print
# fmt_new_hunk=src/theme.rs:42.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

target="${ROOT}/src/theme.rs"
backup="${VERIFY_RUN_DIR}/theme.rs.shift.bak"
log="${VERIFY_EVIDENCE_DIR}/fmt-shift.log"
cp "${target}" "${backup}"
restore() {
  cp "${backup}" "${target}"
}
trap restore EXIT

python3 - "${target}" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
pad = "".join("// pad\n" for _ in range(20))
path.write_text(pad + path.read_text(encoding="utf-8"), encoding="utf-8")
PY

set +e
"${SCRIPT_DIR}/fmt-gate.sh" >"${log}" 2>&1
status=$?
set -e

restore
trap - EXIT
if ! cmp -s "${target}" "${backup}"; then
  echo "restore failed for ${target}" >&2
  exit 1
fi
echo "restored=src/theme.rs"

token="fmt_new_hunk=src/theme.rs:42"
if [[ "${status}" -eq 0 ]]; then
  echo "RED PROOF FAILED fmt-gate.sh exited 0 after shifting src/theme.rs:22 to :42" >&2
  exit 1
fi
if ! grep -Fxq -- "${token}" "${log}"; then
  echo "RED PROOF FAILED log missing exact line ${token}" >&2
  grep -E '^(fmt_|ci_fail=)' "${log}" >&2 || true
  exit 1
fi
if grep -Fxq -- "fmt_gate=pass" "${log}"; then
  echo "RED PROOF FAILED log contains fmt_gate=pass" >&2
  exit 1
fi

echo "RED fmt result=FAIL_AS_REQUIRED gate_exit=${status}"
echo "seed=20 lines // pad"
echo "${token}"
grep -E '^(fmt_hunk_body=|fmt_new_hunk=|fmt_new_file=|fmt_gate=|fmt_baseline_|fmt_observed_|fmt_cargo_status=)' "${log}"
echo "log=${log}"
