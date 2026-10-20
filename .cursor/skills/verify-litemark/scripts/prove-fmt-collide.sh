#!/usr/bin/env bash
# Collision: rustfmt src/model/settings.rs, then put
# fn zz_coincide(){let _x=1;} at line 1. cargo fmt reports that header,
# which is already on the baseline, with a different body.
# The gate must exit 1 and print fmt_hunk_body=src/model/settings.rs:1.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

target_rel="src/model/settings.rs"
target="${ROOT}/${target_rel}"
backup="${VERIFY_RUN_DIR}/settings.rs.collide.bak"
log="${VERIFY_EVIDENCE_DIR}/fmt-collide.log"
cp "${target}" "${backup}"
restore() {
  cp "${backup}" "${target}"
}
trap restore EXIT

rustfmt --edition 2021 "${target}"
python3 - "${target}" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
path.write_text("fn zz_coincide(){let _x=1;}\n" + path.read_text(encoding="utf-8"), encoding="utf-8")
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
echo "restored=${target_rel}"

token="fmt_hunk_body=${target_rel}:1"
if [[ "${status}" -eq 0 ]]; then
  echo "RED PROOF FAILED fmt-gate.sh exited 0 after fn zz_coincide(){let _x=1;} at ${target_rel}:1" >&2
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
echo "seed=fn zz_coincide(){let _x=1;}"
echo "colliding_header=${target_rel}:1"
echo "${token}"
grep -E '^(fmt_hunk_body=|fmt_new_hunk=|fmt_new_file=|fmt_gate=|fmt_baseline_|fmt_observed_|fmt_cargo_status=)' "${log}"
echo "log=${log}"
