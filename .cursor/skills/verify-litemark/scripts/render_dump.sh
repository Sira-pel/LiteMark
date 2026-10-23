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

# Return success when any existing parent component of the output path is a
# symlink. Walk the lexical path rather than resolving it, so the refusal
# names the path the caller supplied. Stop at the first missing component:
# mkdir -p would create everything below it without following a symlink.
refuse_symlink_parent() {
  local out="$1" dir cur part
  local -a parts=()
  dir="$(dirname -- "${out}")"
  case "${dir}" in
    /*) cur="" ;;
    *) cur="${PWD}" ;;
  esac
  IFS='/' read -r -a parts <<< "${dir#/}"
  for part in "${parts[@]}"; do
    [[ -z "${part}" || "${part}" == "." ]] && continue
    if [[ "${part}" == ".." ]]; then
      cur="$(dirname -- "${cur}")"
      continue
    fi
    cur="${cur%/}/${part}"
    if [[ -L "${cur}" ]]; then
      return 0
    fi
    if [[ ! -e "${cur}" ]]; then
      return 1
    fi
  done
  return 1
}

# Follow symlinks for the input inode: the write path resolves argv1 with
# readlink -f, so same_inode must compare the target file, not the symlink node.
input_id="$(stat -L -c '%d:%i' -- "$1")"
input_lexical="$(realpath -m -s -- "$1")"
output_lexical="$(realpath -m -s -- "$2")"
if [[ "${input_lexical}" == "${output_lexical}" ]]; then
  echo "render_refuse=same_as_input path=$2"
  exit 1
fi
if [[ -L "$2" ]]; then
  echo "render_refuse=symlink path=$2"
  exit 1
fi
if refuse_symlink_parent "$2"; then
  echo "render_refuse=symlink_parent path=$2"
  exit 1
fi
if [[ -e "$2" ]]; then
  out_id="$(stat -c '%d:%i' -- "$2")"
  if [[ "${out_id}" == "${input_id}" ]]; then
    echo "render_refuse=same_inode path=$2"
    exit 1
  fi
  while IFS= read -r -d '' oracle; do
    oracle_id="$(stat -c '%d:%i' -- "${oracle}")"
    if [[ "${out_id}" == "${oracle_id}" ]]; then
      echo "render_refuse=same_inode path=$2"
      exit 1
    fi
  done < <(find "${KIT_DIR}/fixtures" -type f \( -name 'expect.txt' -o -name 'required-lines.txt' \) -print0)
fi
mkdir -p "$(dirname "$2")"
input="$(readlink -f "$1")"
output="$(readlink -f "$2")"
# Lexical/lstat checks can miss argv1=symlink → argv2=real; refuse using the
# same resolved paths the dump write will use.
if [[ "${input}" == "${output}" ]]; then
  echo "render_refuse=same_as_input path=$2"
  exit 1
fi
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
