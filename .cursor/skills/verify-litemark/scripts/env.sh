#!/usr/bin/env bash
# Shared environment for control-litemark. Source from anywhere:
#   export RUN_ID=...
#   source .cursor/skills/verify-litemark/scripts/env.sh
set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "verify-litemark helpers run on Linux only." >&2
  return 1 2>/dev/null || exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
cd "${ROOT}"

# Reject before any run path is composed. The charset allows '.' so `..`
# is rejected on its own. `/` is outside the charset.
RUN_ID="${RUN_ID:-$(date +%Y%m%d%H%M%S)}"
if [[ ! "${RUN_ID}" =~ ^[A-Za-z0-9._-]+$ ]] || [[ "${RUN_ID}" == *..* ]]; then
  printf 'run_id_rejected=%s\n' "${RUN_ID}" >&2
  return 1 2>/dev/null || exit 1
fi

VERIFY_RUN_DIR="${VERIFY_RUN_DIR:-/tmp/litemark-verify-${RUN_ID}}"
VERIFY_EVIDENCE_DIR="${VERIFY_EVIDENCE_DIR:-${VERIFY_RUN_DIR}/evidence}"
VERIFY_ARTIFACTS_DIR="${VERIFY_ARTIFACTS_DIR:-${ROOT}/.cursor/skills/verify-litemark/artifacts}"
KIT_DIR="${ROOT}/.cursor/skills/verify-litemark"
LITEMARK_WRAP_COLS="${LITEMARK_WRAP_COLS:-40}"

# cargo metadata honours CARGO_TARGET_DIR. Callers that set LITEMARK_BIN
# keep that value; launch.sh resolves the debug binary itself.
litemark_debug_bin() {
  local meta target_dir
  meta="$(cargo metadata --format-version 1 --no-deps --quiet)"
  target_dir="$(printf '%s\n' "${meta}" | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' | head -n 1)"
  if [[ -z "${target_dir}" ]]; then
    echo "litemark_bin=missing reason=target_directory" >&2
    return 1
  fi
  printf '%s\n' "${target_dir}/debug/litemark"
}

if [[ -z "${LITEMARK_BIN:-}" ]]; then
  LITEMARK_BIN="$(litemark_debug_bin)"
fi

mkdir -p \
  "${VERIFY_RUN_DIR}/logs" \
  "${VERIFY_EVIDENCE_DIR}" \
  "${VERIFY_ARTIFACTS_DIR}"

# main has no rust-toolchain.toml and no .github/workflows.
# Stable is pinned by branch cursor/linux-packaging-readme-4544,
# blob a24e5dd3af5ff2f6b0a7bfdad3530f4e580920db
# (.github/workflows/ci.yml), step dtolnay/rust-toolchain@stable.
# Cargo.lock pins ar_archive_writer 0.5.3 (rust-version 1.88.0),
# which rustc 1.83 cannot parse.
if command -v rustup >/dev/null 2>&1; then
  export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
fi

export RUN_ID VERIFY_RUN_DIR VERIFY_EVIDENCE_DIR VERIFY_ARTIFACTS_DIR
export LITEMARK_BIN ROOT KIT_DIR LITEMARK_WRAP_COLS
