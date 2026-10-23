#!/usr/bin/env bash
# Seed a fmt break in a file that is not on the fmt baseline.
# scripts/ci.sh must exit non-zero and print fmt_new_file=<that path>.
# The seed is removed before this script exits.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

target_rel="src/markdown/render_dump.rs"
target="${ROOT}/${target_rel}"
baseline="${KIT_DIR}/fixtures/fmt-baseline.files"
backup="${VERIFY_RUN_DIR}/render_dump.rs.bak"
log="${VERIFY_EVIDENCE_DIR}/fmt-red-ci.log"

if grep -Fxq -- "${target_rel}" "${baseline}"; then
  echo "RED FIXTURE INVALID ${target_rel} is on ${baseline}" >&2
  exit 1
fi

cp "${target}" "${backup}"
restore() {
  cp "${backup}" "${target}"
}
trap restore EXIT

printf '\n#[allow(dead_code)]\nfn zz_fmt_red(){let _x=1;}\n' >>"${target}"

set +e
"${SCRIPT_DIR}/ci.sh" >"${log}" 2>&1
status=$?
set -e

restore
trap - EXIT

if ! cmp -s "${target}" "${backup}"; then
  echo "restore failed for ${target}" >&2
  exit 1
fi
echo "restored=${target_rel}"

token="fmt_new_file=${target_rel}"
if [[ "${status}" -eq 0 ]]; then
  echo "RED PROOF FAILED ci.sh exited 0 after seeding fn zz_fmt_red(){let _x=1;} in ${target_rel}"
  exit 1
fi
if ! grep -Fxq -- "${token}" "${log}"; then
  echo "RED PROOF FAILED log missing exact line ${token}" >&2
  grep -E '^(fmt_|ci_fail=)' "${log}" >&2 || true
  exit 1
fi

echo "RED fmt result=FAIL_AS_REQUIRED ci_exit=${status}"
echo "seed=fn zz_fmt_red(){let _x=1;}"
echo "file_not_on_baseline=${target_rel}"
echo "${token}"
grep -E '^(fmt_new_file=|fmt_new_hunk=|fmt_gate=|fmt_baseline_|fmt_observed_|fmt_cargo_status=|ci_fail=)' "${log}"
echo "log=${log}"
