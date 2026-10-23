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

# Output path equal to the input path.
same_in="${VERIFY_RUN_DIR}/same-as-input.md"
same_out="${same_in}"
cp "${input}" "${same_in}"
same_before="$(sha256sum "${same_in}")"
set +e
"${SCRIPT_DIR}/render_dump.sh" "${same_in}" "${same_out}" >"${VERIFY_RUN_DIR}/same-as-input.out" 2>&1
same_rc=$?
set -e
if [[ "${same_rc}" -eq 0 ]]; then
  echo "render_dump wrote onto its input" >&2
  exit 1
fi
if ! grep -Fxq "render_refuse=same_as_input path=${same_out}" "${VERIFY_RUN_DIR}/same-as-input.out"; then
  echo "missing same_as_input refuse" >&2
  cat "${VERIFY_RUN_DIR}/same-as-input.out" >&2
  exit 1
fi
same_after="$(sha256sum "${same_in}")"
if [[ "${same_before}" != "${same_after}" ]]; then
  echo "input changed after same_as_input refuse" >&2
  exit 1
fi
echo "render_refuse=same_as_input path=${same_out}"
echo "input_sha_unchanged=1"
echo "proof=P-L1 rc=${same_rc} line=render_refuse=same_as_input path=${same_out}"

# Hardlink of the input used as the output.
hard_in="${VERIFY_RUN_DIR}/hardlink-input.md"
hard_out="${VERIFY_RUN_DIR}/hardlink-input.dump"
cp "${input}" "${hard_in}"
rm -f "${hard_out}"
ln "${hard_in}" "${hard_out}"
hard_in_before="$(sha256sum "${hard_in}")"
set +e
"${SCRIPT_DIR}/render_dump.sh" "${hard_in}" "${hard_out}" >"${VERIFY_RUN_DIR}/hardlink-input.out" 2>&1
hard_in_rc=$?
set -e
if [[ "${hard_in_rc}" -eq 0 ]]; then
  echo "render_dump wrote through an input hardlink" >&2
  exit 1
fi
if ! grep -Fxq "render_refuse=same_inode path=${hard_out}" "${VERIFY_RUN_DIR}/hardlink-input.out"; then
  echo "missing input same_inode refuse" >&2
  cat "${VERIFY_RUN_DIR}/hardlink-input.out" >&2
  exit 1
fi
hard_in_after="$(sha256sum "${hard_in}")"
if [[ "${hard_in_before}" != "${hard_in_after}" ]]; then
  echo "input changed after input hardlink refuse" >&2
  exit 1
fi
rm -f "${hard_out}"
echo "render_refuse=same_inode path=${hard_out}"
echo "input_sha_unchanged=1"
echo "proof=P-L1 rc=${hard_in_rc} line=render_refuse=same_inode path=${hard_out}"

# Output under a symlinked parent directory.
parent_real="${VERIFY_RUN_DIR}/symlink-parent-real"
parent_link="${VERIFY_RUN_DIR}/symlink-parent-link"
parent_target="${parent_real}/target.dump"
parent_out="${parent_link}/target.dump"
rm -rf "${parent_real}" "${parent_link}"
mkdir -p "${parent_real}"
printf 'keep\n' >"${parent_target}"
ln -s "${parent_real}" "${parent_link}"
parent_before="$(sha256sum "${parent_target}")"
set +e
"${SCRIPT_DIR}/render_dump.sh" "${input}" "${parent_out}" >"${VERIFY_RUN_DIR}/symlink-parent.out" 2>&1
parent_rc=$?
set -e
if [[ "${parent_rc}" -eq 0 ]]; then
  echo "render_dump wrote under a symlinked parent" >&2
  exit 1
fi
if ! grep -Fxq "render_refuse=symlink_parent path=${parent_out}" "${VERIFY_RUN_DIR}/symlink-parent.out"; then
  echo "missing symlink_parent refuse" >&2
  cat "${VERIFY_RUN_DIR}/symlink-parent.out" >&2
  exit 1
fi
parent_after="$(sha256sum "${parent_target}")"
if [[ "${parent_before}" != "${parent_after}" ]]; then
  echo "target changed after symlink_parent refuse" >&2
  exit 1
fi
echo "render_refuse=symlink_parent path=${parent_out}"
echo "target_sha_unchanged=1"
echo "proof=P-L1 rc=${parent_rc} line=render_refuse=symlink_parent path=${parent_out}"
