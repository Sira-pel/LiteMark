#!/usr/bin/env bash
# Drop the last pinned id from both files. They still match, and the count
# is below MIN_CASES, so the gate must print count_below_min and exit non-zero.
# Both files are restored before this script exits.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

cases="${KIT_DIR}/fixtures/table-ops/cases.tsv"
ids="${KIT_DIR}/fixtures/table-ops/expected-ids.txt"
cases_bak="${VERIFY_RUN_DIR}/table-ops-cases.drop.bak"
ids_bak="${VERIFY_RUN_DIR}/table-ops-ids.drop.bak"
log="${VERIFY_EVIDENCE_DIR}/table-ops-dropped-case.log"
mkdir -p "${VERIFY_RUN_DIR}" "${VERIFY_EVIDENCE_DIR}"

cp "${cases}" "${cases_bak}"
cp "${ids}" "${ids_bak}"
restore() {
  cp "${cases_bak}" "${cases}"
  cp "${ids_bak}" "${ids}"
}
trap restore EXIT

python3 - "${cases}" "${ids}" <<'PY'
import pathlib, sys
cases = pathlib.Path(sys.argv[1])
ids = pathlib.Path(sys.argv[2])
id_lines = ids.read_text(encoding="utf-8").splitlines()
id_lines = [line for line in id_lines if line.strip()]
dropped = id_lines.pop()
ids.write_text("\n".join(id_lines) + "\n", encoding="utf-8")
kept = []
for line in cases.read_text(encoding="utf-8").splitlines():
    if line.startswith(dropped + "\t"):
        continue
    kept.append(line)
cases.write_text("\n".join(kept) + "\n", encoding="utf-8")
PY

set +e
"${SCRIPT_DIR}/table-ops-gate.sh" >"${log}" 2>&1
status=$?
set -e

restore
trap - EXIT

if ! cmp -s "${cases}" "${cases_bak}" || ! cmp -s "${ids}" "${ids_bak}"; then
  echo "restore failed" >&2
  exit 1
fi
if [[ "${status}" -eq 0 ]]; then
  echo "dropped-case gate exited 0" >&2
  exit 1
fi
if ! grep -q '^count_below_min count=' "${log}"; then
  echo "missing count_below_min" >&2
  exit 1
fi
echo "prove_table_ops_dropped_case=pass"
exit 0
