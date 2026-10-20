#!/usr/bin/env bash
# Run the fixture corpus through the render dump.
#
#   prove-markdown.sh            # green fixtures must match expect.txt
#   prove-markdown.sh green
#   prove-markdown.sh red        # red fixtures must fail their exact line
#   prove-markdown.sh all
#
# Green exit 0: every dump file is byte-identical to its expect.txt.
# Red exit 0: every red fixture's dump does NOT contain missing-line.txt
# as a full line, and that line IS a full line of the named expect file.
# A red exit 0 is the proof the check can fail. It is not a product pass.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

mode="${1:-green}"
ART_DIR="${VERIFY_ARTIFACTS_DIR}/markdown-render/${RUN_ID}"
mkdir -p "${ART_DIR}"

copy_proof() {
  local name="$1"
  local dump="$2"
  local timing="$3"
  cp -f "${dump}" "${ART_DIR}/${name}.dump.txt"
  if [[ -f "${timing}" ]]; then
    cp -f "${timing}" "${ART_DIR}/${name}.timing.txt"
  fi
}

run_green() {
  local name="$1"
  local dir="${KIT_DIR}/fixtures/${name}"
  local input="${dir}/input.md"
  local expect="${dir}/expect.txt"
  local dump="${VERIFY_EVIDENCE_DIR}/${name}.dump.txt"
  local timing="${VERIFY_EVIDENCE_DIR}/${name}.timing.txt"
  local elapsed dump_status
  set +e
  elapsed="$("${SCRIPT_DIR}/render_dump.sh" "${input}" "${dump}" "${timing}")"
  dump_status=$?
  set -e
  if [[ "${dump_status}" -ne 0 ]]; then
    echo "FAIL fixture=${name} harness_exit=${dump_status} input=${input}"
    return 1
  fi
  copy_proof "${name}" "${dump}" "${timing}"
  if cmp -s "${expect}" "${dump}"; then
    echo "PASS fixture=${name} expect=${expect} observed=${dump} artifact=${ART_DIR}/${name}.dump.txt ${elapsed}"
    if [[ "${name}" == "large-doc" ]]; then
      grep -E '^(bytes|lines|wrap_cols)=' "${dump}"
    else
      echo "--- observed ${name} ---"
      cat "${dump}"
      echo "--- end observed ${name} ---"
    fi
    return 0
  fi
  echo "FAIL fixture=${name} expect=${expect} observed=${dump}"
  diff -u "${expect}" "${dump}" || true
  return 1
}

run_red() {
  local name="$1"
  local dir="${KIT_DIR}/fixtures/red/${name}"
  local input="${dir}/input.md"
  local rel
  rel="$(tr -d '\n' <"${dir}/expect.path")"
  local expect="${KIT_DIR}/${rel}"
  local token_file="${dir}/missing-line.txt"
  local dump="${VERIFY_EVIDENCE_DIR}/red-${name}.dump.txt"
  local timing="${VERIFY_EVIDENCE_DIR}/red-${name}.timing.txt"
  if [[ ! -f "${expect}" ]]; then
    echo "RED FIXTURE INVALID fixture=${name} expect missing ${expect}"
    return 1
  fi
  if [[ "$(grep -c '' "${token_file}")" -ne 1 ]]; then
    echo "RED FIXTURE INVALID fixture=${name} missing-line.txt must be one line"
    return 1
  fi
  local token
  IFS= read -r token <"${token_file}"
  if ! grep -Fxq -- "${token}" "${expect}"; then
    echo "RED FIXTURE INVALID fixture=${name} missing-line.txt is not an exact line of ${expect}"
    echo "token=${token}"
    return 1
  fi
  set +e
  "${SCRIPT_DIR}/render_dump.sh" "${input}" "${dump}" "${timing}" >/dev/null
  local dump_status=$?
  set -e
  if [[ "${dump_status}" -ne 0 ]]; then
    echo "RED FIXTURE INVALID fixture=${name} harness_exit=${dump_status} input=${input}"
    return 1
  fi
  copy_proof "red-${name}" "${dump}" "${timing}"
  if grep -Fxq -- "${token}" "${dump}"; then
    echo "RED PROOF FAILED fixture=${name} observed dump still contains the exact line"
    echo "missing_exact_line=${token}"
    echo "observed=${dump}"
    return 1
  fi
  if cmp -s "${expect}" "${dump}"; then
    echo "RED PROOF FAILED fixture=${name} dump matched ${expect}"
    return 1
  fi
  echo "RED fixture=${name} result=FAIL_AS_REQUIRED expect=${expect} observed=${dump} artifact=${ART_DIR}/red-${name}.dump.txt"
  echo "missing_exact_line=${token}"
  echo "--- observed head ${name} ---"
  head -n 20 "${dump}"
  echo "--- end observed head ${name} ---"
  return 0
}

green_names=(
  headings
  emphasis
  nested-lists
  tables
  fenced-code
  links
  images
  long-lines
  large-doc
)
red_names=(
  headings-missing-marker
  emphasis-no-markers
  nested-lists-flat
  tables-not-a-table
  fenced-code-no-fence
  links-plain
  images-plain
  long-lines-short
  large-doc-tiny
)

fail=0
case "${mode}" in
  green)
    for name in "${green_names[@]}"; do
      run_green "${name}" || fail=1
    done
    ;;
  red)
    for name in "${red_names[@]}"; do
      run_red "${name}" || fail=1
    done
    ;;
  all)
    for name in "${green_names[@]}"; do
      run_green "${name}" || fail=1
    done
    for name in "${red_names[@]}"; do
      run_red "${name}" || fail=1
    done
    ;;
  *)
    echo "usage: prove-markdown.sh [green|red|all]" >&2
    exit 2
    ;;
esac

{
  printf 'run_id=%s\n' "${RUN_ID}"
  printf 'mode=%s\n' "${mode}"
  printf 'wrap_cols=%s\n' "${LITEMARK_WRAP_COLS}"
  printf 'fail=%s\n' "${fail}"
  printf 'bin=%s\n' "${LITEMARK_BIN}"
} >"${ART_DIR}/meta-${mode}.txt"

echo "prove_markdown mode=${mode} fail=${fail} artifacts=${ART_DIR}"
exit "${fail}"
