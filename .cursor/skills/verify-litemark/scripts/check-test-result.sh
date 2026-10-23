#!/usr/bin/env bash
# Require one full anchored cargo test summary line with a positive passed count.
# An unanchored substring match can be fooled by a decoy line after a real
# zero-passed summary.
#
# Usage:
#   scripts/check-test-result.sh <test.log>
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: check-test-result.sh <test.log>" >&2
  exit 2
fi

if grep -Eq '^test result: ok\. [1-9][0-9]* passed; ' "$1"; then
  exit 0
fi

echo "test_result_ok=missing"
exit 1
