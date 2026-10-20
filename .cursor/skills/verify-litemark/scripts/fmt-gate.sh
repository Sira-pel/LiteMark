#!/usr/bin/env bash
# Bounded rustfmt gate.
#
# cargo fmt --check may be dirty. That is allowed only for files listed in
# fixtures/fmt-baseline.files and only for hunk headers listed in
# fixtures/fmt-baseline.hunks (repo-relative path:line from `Diff in`).
#
# A file that is not on the list prints fmt_new_file=<path> and fails.
# A hunk header that is not on the list prints fmt_new_hunk=<path:line> and fails.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

files_list="${KIT_DIR}/fixtures/fmt-baseline.files"
hunks_list="${KIT_DIR}/fixtures/fmt-baseline.hunks"
log="${VERIFY_EVIDENCE_DIR}/ci/fmt.log"
observed="${VERIFY_RUN_DIR}/fmt-observed.hunks"
mkdir -p "${VERIFY_EVIDENCE_DIR}/ci"

if [[ ! -f "${files_list}" || ! -f "${hunks_list}" ]]; then
  echo "fmt_gate=fail reason=missing_baseline files=${files_list} hunks=${hunks_list}"
  exit 1
fi

set +e
cargo fmt --check >"${log}" 2>&1
fmt_status=$?
set -e

python3 - "${ROOT}" "${log}" "${observed}" <<'PY'
import sys
from pathlib import Path

root = Path(sys.argv[1]).resolve()
log_path = Path(sys.argv[2])
out_path = Path(sys.argv[3])
hunks = []
for line in log_path.read_text(encoding="utf-8", errors="replace").splitlines():
    if not line.startswith("Diff in ") or not line.endswith(":"):
        continue
    rest = line[len("Diff in ") : -1]
    path_text, lineno = rest.rsplit(":", 1)
    path = Path(path_text)
    if path.is_absolute():
        rel = str(path.resolve().relative_to(root))
    else:
        rel = path_text
    hunks.append(f"{rel}:{lineno}")
out_path.write_text("".join(f"{hunk}\n" for hunk in hunks))
PY

fail=0
observed_n=0
while IFS= read -r hunk || [[ -n "${hunk}" ]]; do
  [[ -n "${hunk}" ]] || continue
  observed_n=$((observed_n + 1))
  file="${hunk%%:*}"
  if ! grep -Fxq -- "${file}" "${files_list}"; then
    echo "fmt_new_file=${file}"
    echo "fmt_new_hunk=${hunk}"
    fail=1
  elif ! grep -Fxq -- "${hunk}" "${hunks_list}"; then
    echo "fmt_new_hunk=${hunk}"
    fail=1
  fi
done <"${observed}"

baseline_files="$(grep -c '' "${files_list}")"
baseline_hunks="$(grep -c '' "${hunks_list}")"
echo "fmt_baseline_files=${baseline_files}"
echo "fmt_baseline_hunks=${baseline_hunks}"
echo "fmt_observed_hunks=${observed_n}"
echo "fmt_cargo_status=${fmt_status}"

if [[ "${fmt_status}" -ne 0 && "${observed_n}" -eq 0 ]]; then
  echo "fmt_gate=fail reason=cargo_fmt_check_exit=${fmt_status} hunks=0"
  exit 1
fi

if [[ "${fail}" -ne 0 ]]; then
  echo "fmt_gate=fail"
  exit 1
fi
echo "fmt_gate=pass"
exit 0
