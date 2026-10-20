#!/usr/bin/env bash
# Show `cargo fmt --check` failing on a seeded unformatted function, then restore.
# Exit 0 only when fmt exits non-zero and names src/markdown/render_dump.rs.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

target="${ROOT}/src/markdown/render_dump.rs"
backup="${VERIFY_RUN_DIR}/render_dump.rs.bak"
log="${VERIFY_EVIDENCE_DIR}/fmt-red.log"
cp "${target}" "${backup}"

restore() {
  cp "${backup}" "${target}"
}
trap restore EXIT

printf '\nfn zz_fmt_red(){let x=1;}\n' >>"${target}"

set +e
cargo fmt --check >"${log}" 2>&1
status=$?
set -e

restore
trap - EXIT

if cmp -s "${target}" "${backup}"; then
  echo "restored=${target}"
else
  echo "restore failed for ${target}" >&2
  exit 1
fi

if [[ "${status}" -eq 0 ]]; then
  echo "RED PROOF FAILED cargo fmt --check exited 0 after seeding fn zz_fmt_red(){let x=1;}"
  exit 1
fi

token="Diff in ${ROOT}/src/markdown/render_dump.rs:"
if ! grep -F "${token}" "${log}" >/dev/null; then
  echo "RED PROOF FAILED log does not contain ${token}" >&2
  cat "${log}" >&2
  exit 1
fi

echo "RED fmt result=FAIL_AS_REQUIRED exit=${status}"
echo "seed=fn zz_fmt_red(){let x=1;}"
echo "missing_exact_path=${token}"
echo "log=${log}"
cat "${log}"
