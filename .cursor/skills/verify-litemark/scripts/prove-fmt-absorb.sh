#!/usr/bin/env bash
# Absorption: a new statement inside the existing src/theme.rs:22 hunk.
# The Diff in header stays src/theme.rs:22. The body changes, so the gate
# must exit 1 and print fmt_hunk_body=src/theme.rs:22.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

target="${ROOT}/src/theme.rs"
backup="${VERIFY_RUN_DIR}/theme.rs.absorb.bak"
log="${VERIFY_EVIDENCE_DIR}/fmt-absorb.log"
cp "${target}" "${backup}"
restore() {
  cp "${backup}" "${target}"
}
trap restore EXIT

python3 - "${target}" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = """            ThemeMode::System => matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark),
        }
    }
"""
new = """            ThemeMode::System => matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark),
        }
        let _new_break=1;
    }
"""
count = text.count(old)
if count != 1:
    raise SystemExit(f"absorb anchor count={count}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY

set +e
"${SCRIPT_DIR}/fmt-gate.sh" >"${log}" 2>&1
status=$?
set -e

restore
trap - EXIT
if ! cmp -s "${target}" "${backup}"; then
  echo "restore failed for ${target}" >&2
  exit 1
fi
echo "restored=src/theme.rs"

token="fmt_hunk_body=src/theme.rs:22"
if [[ "${status}" -eq 0 ]]; then
  echo "RED PROOF FAILED fmt-gate.sh exited 0 after let _new_break=1; inside src/theme.rs:22" >&2
  exit 1
fi
if ! grep -Fxq -- "${token}" "${log}"; then
  echo "RED PROOF FAILED log missing exact line ${token}" >&2
  grep -E '^(fmt_|ci_fail=)' "${log}" >&2 || true
  exit 1
fi
if grep -Fxq -- "fmt_gate=pass" "${log}"; then
  echo "RED PROOF FAILED log contains fmt_gate=pass" >&2
  exit 1
fi

echo "RED fmt result=FAIL_AS_REQUIRED gate_exit=${status}"
echo "seed=let _new_break=1;"
echo "absorbed_header=src/theme.rs:22"
echo "${token}"
grep -E '^(fmt_hunk_body=|fmt_new_hunk=|fmt_new_file=|fmt_gate=|fmt_baseline_|fmt_observed_|fmt_cargo_status=)' "${log}"
echo "log=${log}"
