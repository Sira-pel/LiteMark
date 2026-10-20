#!/usr/bin/env bash
# Read-only check: is the headless harness worth driving?
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

fail=0
say() { printf '%s\n' "$*"; }

rustc_version="$(rustc --version 2>/dev/null || true)"
if [[ -z "${rustc_version}" ]]; then
  say "doctor fail rustc=missing"
  exit 1
fi
say "rustc=${rustc_version}"

if [[ ! -x "${LITEMARK_BIN}" ]]; then
  say "doctor fail bin=${LITEMARK_BIN} missing"
  say "run .cursor/skills/verify-litemark/scripts/launch.sh"
  fail=1
else
  say "bin=${LITEMARK_BIN}"
fi

hook="${ROOT}/src/markdown/render_dump.rs"
if [[ ! -f "${hook}" ]] || ! grep -Fxq 'fn dump_render_tree() {' "${hook}"; then
  say "doctor fail hook=${hook} missing fn dump_render_tree"
  fail=1
else
  say "hook=${hook}"
  say "test=markdown::render_dump::dump_render_tree"
fi

green_fixtures=(
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
for name in "${green_fixtures[@]}"; do
  input="${KIT_DIR}/fixtures/${name}/input.md"
  expect="${KIT_DIR}/fixtures/${name}/expect.txt"
  if [[ ! -f "${input}" ]]; then
    say "doctor fail missing ${input}"
    fail=1
  fi
  if [[ ! -f "${expect}" ]]; then
    say "doctor fail missing ${expect}"
    fail=1
  fi
done

red_fixtures=(
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
for name in "${red_fixtures[@]}"; do
  dir="${KIT_DIR}/fixtures/red/${name}"
  for file in input.md expect.path missing-line.txt; do
    if [[ ! -f "${dir}/${file}" ]]; then
      say "doctor fail missing ${dir}/${file}"
      fail=1
    fi
  done
done

say "wrap_cols=${LITEMARK_WRAP_COLS}"
say "gui=refused notes_dir=dirs::document_dir()/Notes"

if [[ "${fail}" -ne 0 ]]; then
  say "doctor fail"
  exit 1
fi
say "doctor ok"
