#!/usr/bin/env bash
# Run the fixture corpus through the render dump.
#
#   prove-markdown.sh            # green fixtures must match expect.txt
#   prove-markdown.sh green
#   prove-markdown.sh red        # red fixtures must fail their exact line
#   prove-markdown.sh all
#
# Green exit 0: every small dump is byte-identical to expect.txt, and the
# generated large doc matches input.sha256 plus required-lines.txt.
# A wrong large-doc seed exits before any fixture render with green_fail=1.
# Red prints red_caught=<n>/<total> and exits 0 only when every red input
# is caught: broken-line.txt is an exact dump line, and missing-line.txt
# is not. Otherwise it exits non-zero. red_caught is not a product pass.
#   prove-markdown.sh red <fixture>   # one red fixture
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

generate_large_doc() {
  local dir="${KIT_DIR}/fixtures/large-doc"
  local generated="${VERIFY_RUN_DIR}/large-doc/input.md"
  local gen_out gen_status sha_line expected
  mkdir -p "${VERIFY_RUN_DIR}/large-doc"
  set +e
  gen_out="$(python3 "${SCRIPT_DIR}/generate-large-doc.py" "${dir}/seed.txt" "${generated}" 2>&1)"
  gen_status=$?
  set -e
  printf '%s\n' "${gen_out}"
  if [[ "${gen_status}" -ne 0 ]]; then
    return 1
  fi
  if ! printf '%s\n' "${gen_out}" | grep -Fxq -- "sha256_self=ok"; then
    echo "FAIL fixture=large-doc sha256_self=missing"
    return 1
  fi
  expected="$(tr -d '\n' <"${dir}/input.sha256")"
  sha_line="$(printf '%s\n' "${gen_out}" | grep -E '^sha256=[0-9a-f]{64}$')"
  if [[ "${sha_line}" != "${expected}" ]]; then
    echo "FAIL fixture=large-doc sha256_observed=${sha_line} sha256_expected=${expected}"
    return 1
  fi
  echo "sha256_ok=${sha_line} input=${generated}"
  LARGE_INPUT="${generated}"
}

run_green() {
  local name="$1"
  local dir="${KIT_DIR}/fixtures/${name}"
  local input="${dir}/input.md"
  local expect="${dir}/expect.txt"
  local dump="${VERIFY_EVIDENCE_DIR}/${name}.dump.txt"
  local timing="${VERIFY_EVIDENCE_DIR}/${name}.timing.txt"
  if [[ "${name}" == "large-doc" ]]; then
    input="${LARGE_INPUT}"
    expect="${dir}/required-lines.txt"
  fi
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
  if [[ "${name}" == "large-doc" ]]; then
    local missing=0 line
    while IFS= read -r line || [[ -n "${line}" ]]; do
      [[ -n "${line}" ]] || continue
      if ! grep -Fxq -- "${line}" "${dump}"; then
        echo "FAIL fixture=large-doc missing_exact_line=${line} observed=${dump}"
        missing=1
      fi
    done <"${expect}"
    if [[ "${missing}" -ne 0 ]]; then
      return 1
    fi
    echo "PASS fixture=large-doc required=${expect} observed=${dump} artifact=${ART_DIR}/${name}.dump.txt ${elapsed}"
    grep -E '^(bytes|lines|wrap_cols)=' "${dump}"
    return 0
  fi
  if cmp -s "${expect}" "${dump}"; then
    echo "PASS fixture=${name} expect=${expect} observed=${dump} artifact=${ART_DIR}/${name}.dump.txt ${elapsed}"
    echo "--- observed ${name} ---"
    cat "${dump}"
    echo "--- end observed ${name} ---"
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
  local broken_file="${dir}/broken-line.txt"
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
  if [[ ! -f "${broken_file}" ]]; then
    echo "RED FIXTURE INVALID fixture=${name} missing ${broken_file}"
    return 1
  fi
  if [[ "$(grep -c '' "${broken_file}")" -ne 1 ]]; then
    echo "RED FIXTURE INVALID fixture=${name} broken-line.txt must be one line"
    return 1
  fi
  local token broken
  IFS= read -r token <"${token_file}"
  IFS= read -r broken <"${broken_file}"
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
  if ! grep -Fxq -- "${broken}" "${dump}"; then
    echo "RED PROOF FAILED fixture=${name} broken line absent"
    echo "broken_line_absent=${broken}"
    echo "observed=${dump}"
    return 1
  fi
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
  echo "broken_exact_line=${broken}"
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
  leaf-blocks
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
  images-markup
  long-lines-hard-wrap
  trailing-empty
)

only="${2:-}"
if [[ -n "${only}" && "${mode}" != "red" ]]; then
  echo "usage: prove-markdown.sh red <fixture>" >&2
  exit 2
fi
if [[ -n "${only}" ]]; then
  known=0
  for name in "${red_names[@]}"; do
    if [[ "${name}" == "${only}" ]]; then
      known=1
    fi
  done
  if [[ "${known}" -ne 1 ]]; then
    echo "unknown red fixture ${only}" >&2
    exit 2
  fi
  red_names=("${only}")
fi

finish_green_fail() {
  {
    printf 'run_id=%s\n' "${RUN_ID}"
    printf 'mode=%s\n' "${mode}"
    printf 'wrap_cols=%s\n' "${LITEMARK_WRAP_COLS}"
    printf 'bin=%s\n' "${LITEMARK_BIN}"
    printf 'green_fail=%s\n' "${fail}"
  } >"${ART_DIR}/meta-${mode}.txt"
  echo "prove_markdown mode=${mode} green_fail=${fail} artifacts=${ART_DIR}"
  exit "${fail}"
}

fail=0
caught=0
total="${#red_names[@]}"
case "${mode}" in
  green)
    if ! generate_large_doc; then
      fail=1
      finish_green_fail
    fi
    for name in "${green_names[@]}"; do
      run_green "${name}" || fail=1
    done
    ;;
  red)
    for name in "${red_names[@]}"; do
      if run_red "${name}"; then
        caught=$((caught + 1))
      else
        fail=1
      fi
    done
    ;;
  all)
    if ! generate_large_doc; then
      fail=1
      finish_green_fail
    fi
    for name in "${green_names[@]}"; do
      run_green "${name}" || fail=1
    done
    for name in "${red_names[@]}"; do
      if run_red "${name}"; then
        caught=$((caught + 1))
      else
        fail=1
      fi
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
  printf 'bin=%s\n' "${LITEMARK_BIN}"
  if [[ "${mode}" == "green" ]]; then
    printf 'green_fail=%s\n' "${fail}"
  fi
  if [[ "${mode}" == "red" || "${mode}" == "all" ]]; then
    printf 'red_caught=%s/%s\n' "${caught}" "${total}"
  fi
} >"${ART_DIR}/meta-${mode}.txt"

if [[ "${mode}" == "red" || "${mode}" == "all" ]]; then
  echo "red_caught=${caught}/${total}"
  if [[ "${caught}" -ne "${total}" ]]; then
    echo "prove_markdown mode=${mode} red_caught=${caught}/${total} artifacts=${ART_DIR}"
    exit 1
  fi
fi
if [[ "${mode}" == "green" || "${mode}" == "all" ]]; then
  echo "prove_markdown mode=${mode} green_fail=${fail} artifacts=${ART_DIR}"
  exit "${fail}"
fi
echo "prove_markdown mode=${mode} red_caught=${caught}/${total} artifacts=${ART_DIR}"
exit 0
