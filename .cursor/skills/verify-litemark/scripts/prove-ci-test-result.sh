#!/usr/bin/env bash
# A cargo test runner of "true" exits 0 and prints no test result.
# ci.sh must print test_result_ok=missing and exit non-zero.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

target="${ROOT}/src/markdown/render_dump.rs"
config_dir="${ROOT}/.cargo"
config="${config_dir}/config.toml"
backup="${VERIFY_RUN_DIR}/render_dump.rs.runner.bak"
log="${VERIFY_EVIDENCE_DIR}/ci-runner.log"
if [[ -e "${config}" ]]; then
  echo "refusing to overwrite ${config}" >&2
  exit 1
fi
cp "${target}" "${backup}"
host="$(rustc -vV | sed -n 's/^host: //p')"
restore() {
  cp "${backup}" "${target}"
  rm -f "${config}"
  rmdir "${config_dir}" 2>/dev/null || true
}
trap restore EXIT

mkdir -p "${config_dir}"
cat >"${config}" <<EOF
[target.${host}]
runner = "true"
EOF
cat >>"${target}" <<'EOF'

#[test]
fn zz_runner_should_fail() {
    panic!("zz runner");
}
EOF

set +e
"${SCRIPT_DIR}/ci.sh" >"${log}" 2>&1
status=$?
set -e

restore
trap - EXIT
if ! cmp -s "${target}" "${backup}"; then
  echo "restore failed for src/markdown/render_dump.rs" >&2
  exit 1
fi
if [[ -e "${config}" ]]; then
  echo "restore left ${config}" >&2
  exit 1
fi
echo "restored=src/markdown/render_dump.rs"
echo "restored=.cargo/config.toml"

if [[ "${status}" -eq 0 ]]; then
  echo "ci.sh exited 0 with runner=true" >&2
  exit 1
fi
if ! grep -Fxq "test_result_ok=missing" "${log}"; then
  echo "missing test_result_ok=missing" >&2
  grep -E '^(===== test|test_result_ok=|ci_fail=)' "${log}" >&2 || true
  exit 1
fi
if ! grep -Eq '^===== test exit=0 ' "${log}"; then
  echo "cargo test step was not exit 0" >&2
  grep -E '^===== test ' "${log}" >&2 || true
  exit 1
fi

echo "test_result_ok=missing"
grep -E '^(===== test |test_result_ok=|ci_fail=)' "${log}"
echo "proof=M1 rc=${status} line=test_result_ok=missing"
echo "log=${log}"

# A real zero-passed summary followed by a decoy line containing the old
# unanchored substring must still fail closed. Reuse the same check path
# ci.sh uses, without another cargo run.
decoy_log="${VERIFY_EVIDENCE_DIR}/ci-test-result-decoy.log"
cat >"${decoy_log}" <<'EOF'
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
note: decoy test result: ok. 1 passed
EOF
set +e
decoy_out="$("${SCRIPT_DIR}/check-test-result.sh" "${decoy_log}" 2>&1)"
decoy_rc=$?
set -e
if [[ "${decoy_rc}" -eq 0 ]]; then
  echo "decoy line was accepted as a test pass" >&2
  printf '%s\n' "${decoy_out}" >&2
  exit 1
fi
if ! printf '%s\n' "${decoy_out}" | grep -Fxq "test_result_ok=missing"; then
  echo "decoy case missing test_result_ok=missing" >&2
  printf '%s\n' "${decoy_out}" >&2
  exit 1
fi
echo "proof=M1-decoy rc=${decoy_rc} line=test_result_ok=missing"
echo "decoy_log=${decoy_log}"
