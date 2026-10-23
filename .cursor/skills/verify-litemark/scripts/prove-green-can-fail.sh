#!/usr/bin/env bash
# Seed one wrong exact line into headings/expect.txt, run the green harness,
# require a non-zero exit, then restore the expect file.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

expect="${KIT_DIR}/fixtures/headings/expect.txt"
backup="${VERIFY_RUN_DIR}/headings.expect.bak"
log="${VERIFY_EVIDENCE_DIR}/green-can-fail.log"
good='line[0] kind=Heading level=1 source=0..7 display="Title"'
bad='line[0] kind=Heading level=9 source=0..7 display="Title"'

cp "${expect}" "${backup}"
restore() {
  cp "${backup}" "${expect}"
}
trap restore EXIT

python3 - "${expect}" "${good}" "${bad}" <<'PY'
import sys
from pathlib import Path

path, good, bad = sys.argv[1:]
lines = Path(path).read_text(encoding="utf-8").splitlines()
if good not in lines:
    raise SystemExit(f"missing exact line {good}")
out = [bad if line == good else line for line in lines]
Path(path).write_text("\n".join(out) + "\n", encoding="utf-8")
PY

set +e
"${SCRIPT_DIR}/prove-markdown.sh" green >"${log}" 2>&1
status=$?
set -e

restore
trap - EXIT

if ! cmp -s "${expect}" "${backup}"; then
  echo "restore failed for ${expect}" >&2
  exit 1
fi
echo "restored=${expect}"

if [[ "${status}" -eq 0 ]]; then
  echo "GREEN PROOF FAILED prove-markdown.sh green exited 0 with seeded line ${bad}"
  exit 1
fi
if ! grep -Fxq -- "FAIL fixture=headings expect=${expect} observed=${VERIFY_EVIDENCE_DIR}/headings.dump.txt" "${log}"; then
  echo "GREEN PROOF FAILED log missing FAIL fixture=headings line" >&2
  grep -E '^(FAIL|PASS|prove_markdown|sha256_)' "${log}" >&2 || true
  exit 1
fi

echo "GREEN_CAN_FAIL exit=${status}"
echo "seeded_line=${bad}"
echo "restored_line=${good}"
grep -E '^(FAIL fixture=headings|prove_markdown )' "${log}"
echo "log=${log}"
