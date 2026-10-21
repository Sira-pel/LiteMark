#!/usr/bin/env bash
# SIGINT and SIGTERM during prove-red-mutants.sh restore parser.rs and
# text_ops.rs. sha256 before and after the signal match.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

parser="${ROOT}/src/markdown/parser.rs"
text_ops="${ROOT}/src/model/text_ops.rs"
parser_bak="${VERIFY_RUN_DIR}/parser.rs.signal.bak"
text_bak="${VERIFY_RUN_DIR}/text_ops.rs.signal.bak"
cp "${parser}" "${parser_bak}"
cp "${text_ops}" "${text_bak}"
restore_proof() {
  cp "${parser_bak}" "${parser}"
  cp "${text_bak}" "${text_ops}"
}
trap restore_proof EXIT

python3 - "${SCRIPT_DIR}/prove-red-mutants.sh" "${ROOT}" <<'PY'
import hashlib
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

script = Path(sys.argv[1])
root = Path(sys.argv[2])
files = [
    root / "src/markdown/parser.rs",
    root / "src/model/text_ops.rs",
]


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def one(sig: signal.Signals, run_id: str) -> None:
    before = {path: digest(path) for path in files}
    env = os.environ.copy()
    for key in ("VERIFY_RUN_DIR", "VERIFY_EVIDENCE_DIR", "VERIFY_ARTIFACTS_DIR", "LITEMARK_BIN"):
        env.pop(key, None)
    env["RUN_ID"] = run_id
    proc = subprocess.Popen(
        ["bash", str(script)],
        cwd=root,
        env=env,
        start_new_session=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    deadline = time.time() + 180
    changed = False
    try:
        while time.time() < deadline:
            if digest(files[0]) != before[files[0]]:
                changed = True
                break
            if proc.poll() is not None:
                break
            time.sleep(0.01)
        if not changed:
            raise SystemExit(f"parser.rs did not change before {sig.name}")
        os.killpg(proc.pid, sig)
        try:
            proc.wait(timeout=60)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait(timeout=30)
    finally:
        if proc.poll() is None:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait(timeout=30)
    for path in files:
        after = digest(path)
        identical = 1 if after == before[path] else 0
        rel = path.relative_to(root)
        print(
            f"signal={sig.name} file={rel} sha_before={before[path]} sha_after={after} src_byte_identical={identical}"
        )
        if identical != 1:
            raise SystemExit(f"{rel} changed across {sig.name}")


one(signal.SIGINT, "sigint-r3")
one(signal.SIGTERM, "sigterm-r3")
print("proof=M2 rc=0 line=src_byte_identical=1")
PY

restore_proof
trap - EXIT
if ! cmp -s "${parser}" "${parser_bak}" || ! cmp -s "${text_ops}" "${text_bak}"; then
  echo "signal proof left src dirty" >&2
  exit 1
fi
echo "restored=src/markdown/parser.rs"
echo "restored=src/model/text_ops.rs"
