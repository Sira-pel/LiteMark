#!/usr/bin/env bash
# True when LINE is the large-doc hash failure with a 64-hex observed
# digest and EXPECTED (the committed input.sha256 line) on the right.
large_doc_sha_fail_ok() {
  local line="$1"
  local expected="$2"
  [[ "${line}" =~ ^FAIL\ fixture=large-doc\ sha256_observed=sha256=[0-9a-f]{64}\ sha256_expected=${expected}$ ]]
}
