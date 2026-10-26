#!/usr/bin/env bash
# Drive fixtures/table-ops/cases.tsv through DocumentTab.
#
# The case set is fixtures/table-ops/expected-ids.txt. The script exits
# non-zero when cases.tsv does not have exactly those ids, in that order,
# when the cargo log does not have exactly one whole line
# `case=<id> pass|fail` per id, when the result count is below MIN_CASES,
# or when any result is fail. Ids that contain `/` or `..` are rejected.
# Lower MIN_CASES only when a case is removed on purpose.
set -euo pipefail

MIN_CASES=176

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

ids_file="${KIT_DIR}/fixtures/table-ops/expected-ids.txt"
cases_file="${KIT_DIR}/fixtures/table-ops/cases.tsv"
log="${VERIFY_EVIDENCE_DIR}/table-ops-cargo.log"
mkdir -p "${VERIFY_EVIDENCE_DIR}"

read_ids() {
  local file="$1"
  local kind="$2"
  local line id
  local -n out="$3"
  local -A seen=()
  out=()
  while IFS= read -r line || [[ -n "${line}" ]]; do
    if [[ "${kind}" == "tsv" ]]; then
      [[ -z "${line}" || "${line}" == \#* || "${line}" == id$'\t'* ]] && continue
      id="${line%%$'\t'*}"
    else
      [[ -z "${line}" || "${line}" == \#* ]] && continue
      id="${line}"
    fi
    if [[ "${id}" == *..* || "${id}" == */* ]]; then
      echo "table_ops_gate=id_rejected kind=${kind} id=${id}"
      return 1
    fi
    if [[ -n "${seen[${id}]+x}" ]]; then
      echo "table_ops_gate=duplicate kind=${kind} id=${id}"
      return 1
    fi
    seen["${id}"]=1
    out+=("${id}")
  done <"${file}"
}

expected=()
tsv=()
read_ids "${ids_file}" list expected
read_ids "${cases_file}" tsv tsv

missing=()
extra=()
declare -A tsv_set=()
declare -A expected_set=()
for id in "${tsv[@]}"; do
  tsv_set["${id}"]=1
done
for id in "${expected[@]}"; do
  expected_set["${id}"]=1
  if [[ -z "${tsv_set[${id}]+x}" ]]; then
    missing+=("${id}")
  fi
done
for id in "${tsv[@]}"; do
  if [[ -z "${expected_set[${id}]+x}" ]]; then
    extra+=("${id}")
  fi
done

order_diff=0
if [[ "${#expected[@]}" -eq "${#tsv[@]}" ]]; then
  for ((i = 0; i < ${#expected[@]}; i++)); do
    if [[ "${expected[i]}" != "${tsv[i]}" ]]; then
      order_diff=1
      break
    fi
  done
else
  order_diff=1
fi

if [[ "${#missing[@]}" -ne 0 || "${#extra[@]}" -ne 0 || "${order_diff}" -ne 0 || "${#tsv[@]}" -ne "${#expected[@]}" ]]; then
  printf 'table_ops_gate=id_mismatch missing=%s extra=%s tsv=%s expected=%s\n' \
    "$(IFS=,; echo "${missing[*]-}")" \
    "$(IFS=,; echo "${extra[*]-}")" \
    "${#tsv[@]}" \
    "${#expected[@]}"
  exit 1
fi
printf 'table_ops_ids=ok count=%s\n' "${#expected[@]}"

set +e
cargo test --bin litemark table_ops_cases -- --ignored --nocapture --test-threads=1 >"${log}" 2>&1
cargo_status=$?
set -e

pass=0
fail=0
result_lines=0
declare -A result_word=()
duplicate=0
unlisted=()
while IFS= read -r line; do
  if [[ "${line}" =~ ^case=([^[:space:]]+)\ (pass|fail)$ ]]; then
    id="${BASH_REMATCH[1]}"
    word="${BASH_REMATCH[2]}"
    if [[ -n "${result_word[${id}]+x}" ]]; then
      echo "table_ops_gate=duplicate_result id=${id}"
      duplicate=1
      continue
    fi
    if [[ -z "${expected_set[${id}]+x}" ]]; then
      unlisted+=("${id}")
    fi
    result_word["${id}"]="${word}"
    result_lines=$((result_lines + 1))
    case "${word}" in
      pass) pass=$((pass + 1)) ;;
      fail) fail=$((fail + 1)) ;;
    esac
    printf '%s\n' "${line}"
  fi
done <"${log}"

absent=()
for id in "${expected[@]}"; do
  if [[ -z "${result_word[${id}]+x}" ]]; then
    absent+=("${id}")
  fi
done

printf 'pass=%s fail=%s\n' "${pass}" "${fail}"

if [[ "${result_lines}" -lt "${MIN_CASES}" ]]; then
  printf 'count_below_min count=%s min=%s\n' "${result_lines}" "${MIN_CASES}"
  exit 1
fi
if [[ "${#absent[@]}" -ne 0 || "${#unlisted[@]}" -ne 0 || "${duplicate}" -ne 0 || "${result_lines}" -ne "${#expected[@]}" ]]; then
  printf 'table_ops_gate=result_mismatch absent=%s unlisted=%s lines=%s expected=%s cargo_exit=%s log=%s\n' \
    "$(IFS=,; echo "${absent[*]-}")" \
    "$(IFS=,; echo "${unlisted[*]-}")" \
    "${result_lines}" \
    "${#expected[@]}" \
    "${cargo_status}" \
    "${log}"
  exit 1
fi
if [[ "${fail}" -ne 0 || "${cargo_status}" -ne 0 ]]; then
  exit 1
fi
exit 0
