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

litemark_binary() {
  local bin="$1"
  local magic base
  [[ -f "${bin}" && -x "${bin}" ]] || return 1
  base="$(basename -- "${bin}")"
  [[ "${base}" == "litemark" ]] || return 1
  magic="$(od -An -t x1 -N 4 -- "${bin}" | tr -d '[:space:]')"
  [[ "${magic}" == "7f454c46" ]] || return 1
  grep -a -m 1 -F -q --binary-files=text 'litemark' -- "${bin}"
}

if [[ ! -e "${LITEMARK_BIN}" ]]; then
  say "doctor fail bin=${LITEMARK_BIN} missing"
  say "run .cursor/skills/verify-litemark/scripts/launch.sh"
  fail=1
elif ! litemark_binary "${LITEMARK_BIN}"; then
  say "doctor fail bin=${LITEMARK_BIN} not-litemark"
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
  markers-strike
  tables
  fenced-code
  links
  images
  long-lines
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

large_dir="${KIT_DIR}/fixtures/large-doc"
for file in seed.txt input.sha256 required-lines.txt; do
  if [[ ! -f "${large_dir}/${file}" ]]; then
    say "doctor fail missing ${large_dir}/${file}"
    fail=1
  fi
done
if ! grep -Fxq -- 'sha256=7eac2c0a0471e1b121b9a3b8c6a11020b4e2e98cffb26387f1db057596bd092c' "${large_dir}/input.sha256"; then
  say "doctor fail ${large_dir}/input.sha256 is not the committed sha256 line"
  fail=1
fi
if ! grep -Fxq -- 'lines=2001' "${large_dir}/required-lines.txt"; then
  say "doctor fail missing exact line lines=2001"
  fail=1
fi
if [[ ! -f "${KIT_DIR}/fixtures/fmt-baseline.files" || ! -f "${KIT_DIR}/fixtures/fmt-baseline.hunks" ]]; then
  say "doctor fail missing fmt baseline"
  fail=1
fi

red_fixtures=(
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
for name in "${red_fixtures[@]}"; do
  dir="${KIT_DIR}/fixtures/red/${name}"
  for file in input.md expect.path missing-line.txt broken-line.txt; do
    if [[ ! -f "${dir}/${file}" ]]; then
      say "doctor fail missing ${dir}/${file}"
      fail=1
    fi
  done
done
hunks="${KIT_DIR}/fixtures/fmt-baseline.hunks"
if [[ -f "${hunks}" ]] && grep -Ev '^.+:[0-9]+:[0-9a-f]{64}$' "${hunks}" | grep -q .; then
  say "doctor fail fmt baseline hunk is not path:line:sha256"
  fail=1
fi

say "wrap_cols=${LITEMARK_WRAP_COLS}"
say "gui=refused notes_dir=dirs::document_dir()/Notes"

if [[ "${fail}" -ne 0 ]]; then
  say "doctor fail"
  exit 1
fi
say "doctor ok"
