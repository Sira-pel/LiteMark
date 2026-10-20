#!/usr/bin/env bash
# sections=399 must make prove-markdown.sh green exit non-zero with
# green_fail=1 before any fixture render. The generator asserts the hash.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

seed="${KIT_DIR}/fixtures/large-doc/seed.txt"
backup="${VERIFY_RUN_DIR}/large-doc-seed.bak"
log="${VERIFY_EVIDENCE_DIR}/large-doc-seed.log"
cp "${seed}" "${backup}"
restore() {
  cp "${backup}" "${seed}"
}
trap restore EXIT

python3 - "${seed}" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = "sections=400\n"
new = "sections=399\n"
if text.count(old) != 1:
    raise SystemExit(f"seed anchor count={text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY

set +e
"${SCRIPT_DIR}/prove-markdown.sh" green >"${log}" 2>&1
status=$?
set -e

restore
trap - EXIT
if ! cmp -s "${seed}" "${backup}"; then
  echo "restore failed for ${seed}" >&2
  exit 1
fi
echo "restored=fixtures/large-doc/seed.txt"

if [[ "${status}" -eq 0 ]]; then
  echo "RED PROOF FAILED prove-markdown.sh green exited 0 with sections=399" >&2
  exit 1
fi
if grep -q 'unbound variable' "${log}"; then
  echo "RED PROOF FAILED log contains unbound variable" >&2
  exit 1
fi
if grep -q '^PASS fixture=' "${log}"; then
  echo "RED PROOF FAILED a fixture rendered after the bad seed" >&2
  exit 1
fi
if grep -q 'LITEMARK_ELAPSED_US=' "${log}"; then
  echo "RED PROOF FAILED a render ran after the bad seed" >&2
  exit 1
fi
if ! grep -Eq '^prove_markdown mode=green green_fail=1 ' "${log}"; then
  echo "RED PROOF FAILED log missing green_fail=1" >&2
  exit 1
fi
if ! grep -q '^FAIL fixture=large-doc sha256_observed=sha256=' "${log}"; then
  echo "RED PROOF FAILED log missing sha256 mismatch line" >&2
  exit 1
fi

echo "RED seed result=FAIL_AS_REQUIRED prove_exit=${status}"
echo "seed=sections=399"
grep -E '^(sha256_self=|FAIL fixture=large-doc|prove_markdown )' "${log}"
echo "log=${log}"
