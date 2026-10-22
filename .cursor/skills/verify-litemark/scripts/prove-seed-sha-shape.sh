#!/usr/bin/env bash
# The large-doc sha check is a full 64-hex observed digest plus the
# committed input.sha256 line. sha256=zzzz and sha256=00 do not match.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"
# shellcheck source=sha-fail-line.sh
source "${SCRIPT_DIR}/sha-fail-line.sh"

expected="$(tr -d '[:space:]' <"${KIT_DIR}/fixtures/large-doc/input.sha256")"
reject_line() {
  local observed="$1"
  local line="FAIL fixture=large-doc sha256_observed=${observed} sha256_expected=${expected}"
  if large_doc_sha_fail_ok "${line}" "${expected}"; then
    echo "sha shape accepted ${observed}" >&2
    exit 1
  fi
  echo "sha_shape_reject=${observed}"
}

reject_line "sha256=zzzz"
reject_line "sha256=00"

work="$(mktemp -d)"
cp "${KIT_DIR}/fixtures/large-doc/seed.txt" "${work}/seed.txt"
cp "${KIT_DIR}/fixtures/large-doc/input.sha256" "${work}/input.sha256"
python3 - "${work}/seed.txt" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = "sections=400\n"
if text.count(old) != 1:
    raise SystemExit(f"seed anchor count={text.count(old)}")
path.write_text(text.replace(old, "sections=399\n", 1), encoding="utf-8")
PY

gen_log="${work}/generate.log"
set +e
python3 "${SCRIPT_DIR}/generate-large-doc.py" "${work}/seed.txt" "${work}/out.md" >"${gen_log}" 2>&1
gen_status=$?
set -e
real_line="$(grep -E '^FAIL fixture=large-doc ' "${gen_log}" | head -n 1 || true)"
if [[ "${gen_status}" -eq 0 ]] || ! large_doc_sha_fail_ok "${real_line}" "${expected}"; then
  echo "real sections=399 line was not a 64-hex mismatch" >&2
  printf 'gen_status=%s\n' "${gen_status}" >&2
  printf 'real_line=%s\n' "${real_line}" >&2
  exit 1
fi
observed="${real_line#*sha256_observed=}"
observed="${observed%% *}"
echo "sha_shape_ok=${observed}"
echo "proof=P-L5 rc=0 line=sha_shape_reject=sha256=zzzz"
echo "proof=P-L5 rc=0 line=sha_shape_reject=sha256=00"
echo "proof=P-L5 rc=${gen_status} line=${real_line}"
