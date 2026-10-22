#!/usr/bin/env bash
# render_dump.sh refuses a symlink output and a hardlink of an oracle.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

oracle="${KIT_DIR}/fixtures/headings/expect.txt"
input="${KIT_DIR}/fixtures/headings/input.md"
before="$(sha256sum "${oracle}")"
link="${VERIFY_RUN_DIR}/symlink-expect.dump"
rm -f "${link}"
ln -s "${oracle}" "${link}"

set +e
"${SCRIPT_DIR}/render_dump.sh" "${input}" "${link}" >"${VERIFY_RUN_DIR}/symlink.out" 2>&1
link_rc=$?
set -e
if [[ "${link_rc}" -eq 0 ]]; then
  echo "render_dump wrote through a symlink" >&2
  exit 1
fi
if ! grep -Fxq "render_refuse=symlink path=${link}" "${VERIFY_RUN_DIR}/symlink.out"; then
  echo "missing symlink refuse" >&2
  cat "${VERIFY_RUN_DIR}/symlink.out" >&2
  exit 1
fi
after="$(sha256sum "${oracle}")"
if [[ "${before}" != "${after}" ]]; then
  echo "oracle changed after symlink refuse" >&2
  exit 1
fi
echo "render_refuse=symlink path=${link}"
echo "proof=P-L1 rc=${link_rc} line=render_refuse=symlink path=${link}"

hard="${KIT_DIR}/fixtures/headings/probe-hardlink.dump"
rm -f "${hard}"
ln "${oracle}" "${hard}"
cleanup_hard() {
  rm -f "${hard}"
}
trap cleanup_hard EXIT
set +e
"${SCRIPT_DIR}/render_dump.sh" "${input}" "${hard}" >"${VERIFY_RUN_DIR}/hardlink.out" 2>&1
hard_rc=$?
set -e
cleanup_hard
trap - EXIT
if [[ "${hard_rc}" -eq 0 ]]; then
  echo "render_dump wrote onto an oracle inode" >&2
  exit 1
fi
if ! grep -Fxq "render_refuse=same_inode path=${hard}" "${VERIFY_RUN_DIR}/hardlink.out"; then
  echo "missing same_inode refuse" >&2
  cat "${VERIFY_RUN_DIR}/hardlink.out" >&2
  exit 1
fi
after="$(sha256sum "${oracle}")"
if [[ "${before}" != "${after}" ]]; then
  echo "oracle changed after same_inode refuse" >&2
  exit 1
fi
echo "render_refuse=same_inode path=${hard}"
echo "oracle_sha_unchanged=1"
echo "proof=P-L1 rc=${hard_rc} line=render_refuse=same_inode path=${hard}"
