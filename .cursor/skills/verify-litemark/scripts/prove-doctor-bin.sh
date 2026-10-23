#!/usr/bin/env bash
# LITEMARK_BIN=/bin/true is an executable. Doctor must reject it.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
log="$(mktemp)"
set +e
LITEMARK_BIN=/bin/true \
  VERIFY_RUN_DIR= \
  VERIFY_EVIDENCE_DIR= \
  bash --noprofile --norc "${SCRIPT_DIR}/doctor.sh" >"${log}" 2>&1
status=$?
set -e
if [[ "${status}" -eq 0 ]]; then
  echo "doctor accepted /bin/true" >&2
  exit 1
fi
if ! grep -Fxq "doctor fail bin=/bin/true not-litemark" "${log}"; then
  echo "missing doctor fail line" >&2
  cat "${log}" >&2
  exit 1
fi
echo "doctor fail bin=/bin/true not-litemark"
echo "proof=P-L4 rc=${status} line=doctor fail bin=/bin/true not-litemark"
echo "log=${log}"
