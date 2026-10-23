#!/usr/bin/env bash
# A python3 on PATH that exits 0 makes fmt-gate.sh exit 0 with no
# fmt_gate=pass line. ci.sh must print fmt_gate_pass=missing.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

target="${ROOT}/src/markdown/render_dump.rs"
backup="${VERIFY_RUN_DIR}/render_dump.rs.passline.bak"
log="${VERIFY_EVIDENCE_DIR}/fmt-gate-pass-line.log"
stub="$(mktemp -d)"
cp "${target}" "${backup}"
restore() {
  cp "${backup}" "${target}"
  rm -rf "${stub}"
}
trap restore EXIT

printf '\n#[allow(dead_code)]\nfn zz_fmt_red(){let _x=1;}\n' >>"${target}"
cat >"${stub}/python3" <<'EOF'
#!/bin/sh
exit 0
EOF
chmod +x "${stub}/python3"

set +e
PATH="${stub}:${PATH}" "${SCRIPT_DIR}/ci.sh" >"${log}" 2>&1
status=$?
set -e

restore
trap - EXIT
if ! cmp -s "${target}" "${backup}"; then
  echo "restore failed for src/markdown/render_dump.rs" >&2
  exit 1
fi
echo "restored=src/markdown/render_dump.rs"

if [[ "${status}" -eq 0 ]]; then
  echo "ci.sh exited 0 without fmt_gate=pass" >&2
  exit 1
fi
if ! grep -Fxq "fmt_gate_pass=missing" "${log}"; then
  echo "missing fmt_gate_pass=missing" >&2
  grep -E '^(fmt_|ci_fail=|===== fmt)' "${log}" >&2 || true
  exit 1
fi

echo "fmt_gate_pass=missing"
grep -E '^(fmt_gate_pass=missing|===== fmt exit=|ci_fail=)' "${log}"
echo "proof=P-L2 rc=${status} line=fmt_gate_pass=missing"
echo "log=${log}"
