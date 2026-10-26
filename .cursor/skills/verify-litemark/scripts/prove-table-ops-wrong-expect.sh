#!/usr/bin/env bash
# Flip one expected byte and require case=insert-row-below-base fail.
# The fixture is restored before this script exits.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

cases="${KIT_DIR}/fixtures/table-ops/cases.tsv"
backup="${VERIFY_RUN_DIR}/table-ops-cases.tsv.bak"
log="${VERIFY_EVIDENCE_DIR}/table-ops-wrong-expect.log"
mkdir -p "${VERIFY_RUN_DIR}" "${VERIFY_EVIDENCE_DIR}"

cp "${cases}" "${backup}"
restore() {
  cp "${backup}" "${cases}"
}
trap restore EXIT

python3 - "${cases}" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
lines = path.read_text(encoding="utf-8").splitlines()
out = []
flipped = False
for line in lines:
    if line.startswith("insert-row-below-base\t") and not flipped:
        fields = line.split("\t")
        fields[5] = fields[5].replace("| A |", "| Z |", 1)
        if fields[5] == line.split("\t")[5]:
            raise SystemExit("flip target missing")
        line = "\t".join(fields)
        flipped = True
    out.append(line)
if not flipped:
    raise SystemExit("row missing")
path.write_text("\n".join(out) + "\n", encoding="utf-8")
PY

set +e
"${SCRIPT_DIR}/table-ops-gate.sh" >"${log}" 2>&1
status=$?
set -e

restore
trap - EXIT

if ! cmp -s "${cases}" "${backup}"; then
  echo "restore failed for ${cases}" >&2
  exit 1
fi
if [[ "${status}" -eq 0 ]]; then
  echo "wrong-expect gate exited 0" >&2
  exit 1
fi
if ! grep -Fxq 'case=insert-row-below-base fail' "${log}"; then
  echo "missing case=insert-row-below-base fail" >&2
  exit 1
fi
echo "prove_table_ops_wrong_expect=pass"
exit 0
