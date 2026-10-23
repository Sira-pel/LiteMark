#!/usr/bin/env bash
# Stop a pid this run started. Never delete evidence or artifacts.
# logs are removed only when they are exactly $run_root/logs.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "${SCRIPT_DIR}/env.sh"

refuse() {
  printf 'cleanup_refuse path=%s\n' "$1"
  exit 1
}

run_root="/tmp/litemark-verify-${RUN_ID}"
if [[ "${VERIFY_RUN_DIR}" != "${run_root}" ]]; then
  refuse "${VERIFY_RUN_DIR}"
fi
if [[ -L "${run_root}" ]]; then
  refuse "${run_root}"
fi
run_real="$(readlink -f -- "${run_root}")"
if [[ "${run_real}" != "${run_root}" ]]; then
  refuse "${VERIFY_RUN_DIR}"
fi

pidfile="${VERIFY_RUN_DIR}/app.pid"
if [[ -f "${pidfile}" ]]; then
  pid="$(tr -d '[:space:]' <"${pidfile}")"
  if [[ "${pid}" =~ ^[0-9]+$ ]] && kill -0 "${pid}" 2>/dev/null; then
    kill "${pid}" 2>/dev/null || true
    wait "${pid}" 2>/dev/null || true
    echo "stopped pid=${pid}"
  else
    echo "pidfile present but process not running pid=${pid}"
  fi
else
  echo "no app.pid (headless harness does not start the GUI)"
fi

logs="${run_root}/logs"
if [[ -L "${logs}" ]]; then
  refuse "${logs}"
fi
if [[ -e "${logs}" ]]; then
  logs_real="$(readlink -f -- "${logs}")"
  if [[ "${logs_real}" != "${logs}" ]]; then
    refuse "${logs}"
  fi
fi
rm -rf -- "${logs}"

if [[ ! -d "${VERIFY_EVIDENCE_DIR}" ]]; then
  echo "cleanup ate evidence at ${VERIFY_EVIDENCE_DIR}" >&2
  exit 1
fi
if [[ ! -d "${VERIFY_ARTIFACTS_DIR}" ]]; then
  echo "cleanup ate artifacts at ${VERIFY_ARTIFACTS_DIR}" >&2
  exit 1
fi

echo "evidence remains at ${VERIFY_EVIDENCE_DIR}"
echo "artifacts remain at ${VERIFY_ARTIFACTS_DIR}"
