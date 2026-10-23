#!/usr/bin/env bash
# Bounded rustfmt gate, keyed on hunk content.
#
# Each `Diff in` hunk is `path:line:sha256`. The line keeps a pure shift red.
# The sha256 is the hunk body with line-number fields normalised, so a new
# break inside an allowlisted header (absorption) or a new body that reuses
# an allowlisted header (collision) is `fmt_hunk_body=<path:line>` and fails.
#
# A file that is not on fixtures/fmt-baseline.files prints fmt_new_file=<path>.
# A header that is not on the baseline prints fmt_new_hunk=<path:line>.
# A baseline token left unmatched prints fmt_baseline_stale=<n> and fails.
#
# cargo fmt is pinned to rustfmt.toml in this kit via --config-path, so a
# user or global rustfmt.toml is not read.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

files_list="${KIT_DIR}/fixtures/fmt-baseline.files"
hunks_list="${KIT_DIR}/fixtures/fmt-baseline.hunks"
rustfmt_config="${KIT_DIR}/rustfmt.toml"
log="${VERIFY_EVIDENCE_DIR}/ci/fmt.log"
mkdir -p "${VERIFY_EVIDENCE_DIR}/ci"

if [[ ! -f "${files_list}" || ! -f "${hunks_list}" || ! -f "${rustfmt_config}" ]]; then
  echo "fmt_gate=fail reason=missing_baseline files=${files_list} hunks=${hunks_list} config=${rustfmt_config}"
  exit 1
fi

set +e
cargo fmt --check -- --config-path "${rustfmt_config}" >"${log}" 2>&1
fmt_status=$?
set -e

python3 "${SCRIPT_DIR}/fmt-hunks.py" gate \
  "${ROOT}" "${log}" "${files_list}" "${hunks_list}" "${fmt_status}"
