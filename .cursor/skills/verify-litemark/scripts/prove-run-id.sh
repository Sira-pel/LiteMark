#!/usr/bin/env bash
# RUN_ID with '/', '..', or any other character outside [A-Za-z0-9._-]
# is rejected before a run path is built. cleanup refuses to rm a
# VERIFY_RUN_DIR that is not /tmp/litemark-verify-$RUN_ID.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_SH="${SCRIPT_DIR}/env.sh"

canary="/tmp/canary-victim"
rm -rf "${canary}"
mkdir -p "${canary}/logs" "${canary}/notlogs"
printf 'secret\n' >"${canary}/logs/secret.txt"
printf 'secret\n' >"${canary}/notlogs/secret.txt"
secret_sha="$(sha256sum "${canary}/logs/secret.txt" "${canary}/notlogs/secret.txt")"

reject_id() {
  local id="$1"
  local output rc
  set +e
  output="$(
    RUN_ID="${id}" \
      VERIFY_RUN_DIR= \
      VERIFY_EVIDENCE_DIR= \
      LITEMARK_BIN= \
      bash --noprofile --norc -c "source '${ENV_SH}'" 2>&1
  )"
  rc=$?
  set -e
  if [[ "${rc}" -eq 0 ]]; then
    echo "RUN_ID was accepted: ${id}" >&2
    exit 1
  fi
  if ! printf '%s\n' "${output}" | grep -Fxq "run_id_rejected=${id}"; then
    echo "missing run_id_rejected for ${id}" >&2
    printf '%s\n' "${output}" >&2
    exit 1
  fi
  echo "run_id_rejected=${id}"
  echo "proof=M3 rc=${rc} line=run_id_rejected=${id}"
}

reject_id "/../canary-victim"
reject_id "../canary-victim"
reject_id "../../../home/ubuntu"
reject_id "has space"

now="$(sha256sum "${canary}/logs/secret.txt" "${canary}/notlogs/secret.txt")"
if [[ "${now}" != "${secret_sha}" ]]; then
  echo "canary changed during RUN_ID rejection" >&2
  exit 1
fi

set +e
cleanup_out="$(
  RUN_ID="r3cleanup" \
    VERIFY_EVIDENCE_DIR= \
    LITEMARK_BIN= \
    VERIFY_RUN_DIR="${canary}" \
    bash --noprofile --norc "${SCRIPT_DIR}/cleanup.sh" 2>&1
)"
cleanup_rc=$?
set -e
if [[ "${cleanup_rc}" -eq 0 ]]; then
  echo "cleanup accepted a run dir outside the run root" >&2
  exit 1
fi
if ! printf '%s\n' "${cleanup_out}" | grep -Fxq "cleanup_refuse path=${canary}"; then
  echo "missing cleanup_refuse" >&2
  printf '%s\n' "${cleanup_out}" >&2
  exit 1
fi
now="$(sha256sum "${canary}/logs/secret.txt" "${canary}/notlogs/secret.txt")"
if [[ "${now}" != "${secret_sha}" ]]; then
  echo "canary changed during cleanup" >&2
  exit 1
fi
echo "cleanup_refuse path=${canary}"
echo "canary_survived=1"
echo "proof=M3 rc=${cleanup_rc} line=cleanup_refuse path=${canary}"
