#!/usr/bin/env bash
# A user rustfmt.toml with max_width=40 must not change the gate.
# The clean tip stays fmt_gate=pass. The fmt red fixtures still fail.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

home_dir="${VERIFY_RUN_DIR}/hostile-home"
mkdir -p "${home_dir}/.config/rustfmt"
cat >"${home_dir}/.config/rustfmt/rustfmt.toml" <<'EOF'
max_width = 40
newline_style = "Unix"
EOF

export HOME="${home_dir}"
unset XDG_CONFIG_HOME

clean_log="${VERIFY_EVIDENCE_DIR}/fmt-user-config-clean.log"
set +e
"${SCRIPT_DIR}/fmt-gate.sh" >"${clean_log}" 2>&1
clean_rc=$?
set -e
if [[ "${clean_rc}" -ne 0 ]]; then
  echo "clean tip failed under max_width=40" >&2
  cat "${clean_log}" >&2
  exit 1
fi
if ! grep -Fxq "fmt_gate=pass" "${clean_log}"; then
  echo "missing fmt_gate=pass under max_width=40" >&2
  exit 1
fi
if ! grep -Fxq "fmt_observed_hunks=201" "${clean_log}"; then
  echo "observed hunks changed under max_width=40" >&2
  grep '^fmt_' "${clean_log}" >&2 || true
  exit 1
fi
if ! grep -Fxq "fmt_baseline_stale=0" "${clean_log}"; then
  echo "missing fmt_baseline_stale=0" >&2
  exit 1
fi
echo "fmt_gate=pass"
echo "fmt_observed_hunks=201"
echo "fmt_baseline_stale=0"
echo "proof=M4 rc=${clean_rc} line=fmt_gate=pass"

run_red() {
  local name="$1"
  local token="$2"
  local log="${VERIFY_EVIDENCE_DIR}/fmt-user-config-${name}.log"
  set +e
  "${SCRIPT_DIR}/${name}" >"${log}" 2>&1
  local status=$?
  set -e
  if [[ "${status}" -ne 0 ]]; then
    echo "${name} proof exited ${status} under max_width=40" >&2
    tail -n 40 "${log}" >&2 || true
    exit 1
  fi
  if ! grep -Fxq "${token}" "${log}"; then
    echo "${name} missing ${token}" >&2
    grep -E '^(fmt_|RED |ci_fail=|proof=)' "${log}" >&2 || true
    exit 1
  fi
  if ! grep -Eq 'gate_exit=1|ci_exit=1|proof=L-3 rc=1' "${log}"; then
    echo "${name} did not record rc 1" >&2
    exit 1
  fi
  echo "proof=M4 script=${name} rc=1 line=${token}"
}

run_red prove-fmt-absorb.sh "fmt_hunk_body=src/theme.rs:22"
run_red prove-fmt-collide.sh "fmt_hunk_body=src/model/settings.rs:1"
run_red prove-fmt-shift.sh "fmt_new_hunk=src/theme.rs:42"
run_red prove-fmt-red.sh "fmt_new_file=src/markdown/render_dump.rs"
run_red prove-fmt-stale.sh "fmt_baseline_stale=1"
