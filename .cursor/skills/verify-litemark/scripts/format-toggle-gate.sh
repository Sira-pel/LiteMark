#!/usr/bin/env bash
# Drive fixtures/format-toggle/cases.tsv through DocumentTab.
#
# Prints one `case=<id> pass|fail|xfail|xpass` line per row, then
# `pass=N fail=N xfail=N xpass=N`. Exits non-zero on any fail or xpass.
# known_fail_main rows that still match main_bytes are xfail, so main
# stays green. A known_fail_main row that matches the target is xpass.
# Any other mismatch is fail.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

log="${VERIFY_EVIDENCE_DIR}/format-toggle-cargo.log"
mkdir -p "${VERIFY_EVIDENCE_DIR}"

set +e
cargo test --bin litemark format_toggle_cases -- --ignored --nocapture --test-threads=1 >"${log}" 2>&1
cargo_status=$?
set -e

pass=0
fail=0
xfail=0
xpass=0
# cargo's test header shares a line with the first println. Extract the
# token wherever it sits so that row is still printed and counted.
while IFS= read -r line; do
  case "${line}" in
    case=*\ pass)
      pass=$((pass + 1))
      printf '%s\n' "${line}"
      ;;
    case=*\ fail)
      fail=$((fail + 1))
      printf '%s\n' "${line}"
      ;;
    case=*\ xfail)
      xfail=$((xfail + 1))
      printf '%s\n' "${line}"
      ;;
    case=*\ xpass)
      xpass=$((xpass + 1))
      printf '%s\n' "${line}"
      ;;
  esac
done < <(grep -oE 'case=[^[:space:]]+ (pass|fail|xfail|xpass)' "${log}" || true)

printf 'pass=%s fail=%s xfail=%s xpass=%s\n' "${pass}" "${fail}" "${xfail}" "${xpass}"

if [[ "${pass}" -eq 0 && "${fail}" -eq 0 && "${xfail}" -eq 0 && "${xpass}" -eq 0 ]]; then
  echo "format_toggle_gate=missing cargo_exit=${cargo_status} log=${log}"
  exit 1
fi
if [[ "${fail}" -ne 0 || "${xpass}" -ne 0 || "${cargo_status}" -ne 0 ]]; then
  exit 1
fi
exit 0
