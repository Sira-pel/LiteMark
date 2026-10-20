#!/usr/bin/env bash
# For each red fixture, apply the renderer mutant in scripts/red-mutants.py,
# run that fixture alone, and require prove-markdown.sh to exit non-zero
# because broken-line.txt is absent. Restore the source before the next one.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

fixtures=(
  headings-missing-marker
  emphasis-no-markers
  nested-lists-flat
  tables-not-a-table
  fenced-code-no-fence
  links-plain
  images-markup
  long-lines-hard-wrap
  trailing-empty
)

fail=0
for fixture in "${fixtures[@]}"; do
  mapfile -t meta < <(python3 "${SCRIPT_DIR}/red-mutants.py" label "${fixture}" "${ROOT}")
  label="${meta[0]}"
  rel="${meta[1]}"
  target="${ROOT}/${rel}"
  backup="${VERIFY_RUN_DIR}/mutant-${fixture}.bak"
  log="${VERIFY_EVIDENCE_DIR}/mutant-${fixture}.log"
  broken="$(tr -d '\n' <"${KIT_DIR}/fixtures/red/${fixture}/broken-line.txt")"
  cp "${target}" "${backup}"
  python3 "${SCRIPT_DIR}/red-mutants.py" apply "${fixture}" "${ROOT}"
  set +e
  "${SCRIPT_DIR}/prove-markdown.sh" red "${fixture}" >"${log}" 2>&1
  status=$?
  set -e
  cp "${backup}" "${target}"
  if ! cmp -s "${target}" "${backup}"; then
    echo "restore failed for ${rel}" >&2
    exit 1
  fi
  token="broken_line_absent=${broken}"
  if [[ "${status}" -eq 0 ]]; then
    echo "MUTANT fixture=${fixture} mutant=${label} rc=${status} result=STILL_GREEN" >&2
    fail=1
    continue
  fi
  if ! grep -Fxq -- "${token}" "${log}"; then
    echo "MUTANT fixture=${fixture} mutant=${label} rc=${status} result=MISSING_TOKEN" >&2
    grep -E '^(RED|broken_|red_caught=|prove_markdown|error)' "${log}" >&2 || true
    fail=1
    continue
  fi
  echo "MUTANT fixture=${fixture} mutant=${label} rc=${status} ${token}"
done

if [[ "${fail}" -ne 0 ]]; then
  echo "prove_red_mutants=fail"
  exit 1
fi
echo "prove_red_mutants=pass"
