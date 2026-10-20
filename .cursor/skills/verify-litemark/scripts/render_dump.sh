#!/usr/bin/env bash
# Feed one markdown file through parse_document + visual_rows.
#
# Usage:
#   scripts/render_dump.sh <input.md> <output.dump.txt> [timing.txt]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

if [[ $# -lt 2 ]]; then
  echo "usage: render_dump.sh <input.md> <output.dump.txt> [timing.txt]" >&2
  exit 2
fi

if [[ ! -f "$1" ]]; then
  echo "missing input $1" >&2
  exit 1
fi
mkdir -p "$(dirname "$2")"
input="$(readlink -f "$1")"
output="$(readlink -f "$2")"
timing="${3:-}"

log="${VERIFY_RUN_DIR}/logs/dump-$(basename "$(dirname "${input}")")-$(basename "${input}").log"
set +e
LITEMARK_RENDER_IN="${input}" \
  LITEMARK_RENDER_OUT="${output}" \
  LITEMARK_WRAP_COLS="${LITEMARK_WRAP_COLS}" \
  cargo test --bin litemark markdown::render_dump::dump_render_tree -- --ignored --exact --nocapture \
  >"${log}" 2>&1
status=$?
set -e
if [[ "${status}" -ne 0 || ! -f "${output}" ]]; then
  echo "render dump failed for ${input} (exit ${status})" >&2
  cat "${log}" >&2
  exit 1
fi

elapsed="$(grep -E '^LITEMARK_ELAPSED_US=[0-9]+$' "${log}" | tail -n 1 || true)"
if [[ -z "${elapsed}" ]]; then
  echo "render dump wrote ${output} but log has no LITEMARK_ELAPSED_US line" >&2
  cat "${log}" >&2
  exit 1
fi
if [[ -n "${timing}" ]]; then
  mkdir -p "$(dirname "${timing}")"
  printf '%s\n' "${elapsed}" >"${timing}"
fi
printf '%s\n' "${elapsed}"
