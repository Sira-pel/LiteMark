#!/usr/bin/env python3
"""Identity for one `cargo fmt --check` hunk.

A hunk token is `path:line:sha256`. `path:line` is the `Diff in` header, so a
shift to a new line is a new token. The sha256 is the hunk body with ANSI
stripped and unified-diff `@@` line numbers folded to `@@`, so the hash is
the changed text. A body edit under an old header is a new token too.
"""

from __future__ import annotations

import hashlib
import re
import sys
from collections import Counter
from pathlib import Path

ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
HUNK_AT = re.compile(r"@@ -\d+(?:,\d+)? \+\d+(?:,\d+)? @@(.*)$")


def normalize_body(body: str) -> str:
    body = body.replace("\x0f", "")
    body = ANSI.sub("", body)
    out: list[str] = []
    for line in body.splitlines():
        match = HUNK_AT.match(line)
        if match:
            out.append("@@" + match.group(1))
        else:
            out.append(line)
    if not out:
        return ""
    return "\n".join(out) + "\n"


def parse_hunks(log_text: str, root: Path) -> list[tuple[str, str, str]]:
    lines = log_text.splitlines()
    starts = [
        index
        for index, line in enumerate(lines)
        if line.startswith("Diff in ") and line.endswith(":")
    ]
    root = root.resolve()
    hunks: list[tuple[str, str, str]] = []
    for nth, start in enumerate(starts):
        end = starts[nth + 1] if nth + 1 < len(starts) else len(lines)
        header = lines[start]
        rest = header[len("Diff in ") : -1]
        path_text, lineno = rest.rsplit(":", 1)
        path = Path(path_text)
        if path.is_absolute():
            rel = str(path.resolve().relative_to(root))
        else:
            rel = path_text
        body = normalize_body("\n".join(lines[start + 1 : end]))
        digest = hashlib.sha256(body.encode("utf-8")).hexdigest()
        hunks.append((rel, lineno, digest))
    return hunks


def token(rel: str, lineno: str, digest: str) -> str:
    return f"{rel}:{lineno}:{digest}"


def header_of(item: str) -> str:
    rel, lineno, _digest = item.rsplit(":", 2)
    return f"{rel}:{lineno}"


def file_of(item: str) -> str:
    rel, _lineno, _digest = item.rsplit(":", 2)
    return rel


def gate(root: Path, log_path: Path, files_path: Path, hunks_path: Path, fmt_status: int) -> int:
    observed = [
        token(rel, lineno, digest) for rel, lineno, digest in parse_hunks(log_path.read_text(encoding="utf-8", errors="replace"), root)
    ]
    files = [line for line in files_path.read_text(encoding="utf-8").splitlines() if line]
    baseline = [line for line in hunks_path.read_text(encoding="utf-8").splitlines() if line]
    file_set = set(files)
    baseline_counts = Counter(baseline)
    baseline_headers = {header_of(item) for item in baseline}
    fail = 0
    for item in observed:
        rel = file_of(item)
        header = header_of(item)
        if rel not in file_set:
            print(f"fmt_new_file={rel}")
            print(f"fmt_new_hunk={header}")
            fail = 1
            continue
        if baseline_counts[item] > 0:
            baseline_counts[item] -= 1
            continue
        if header in baseline_headers:
            print(f"fmt_hunk_body={header}")
        else:
            print(f"fmt_new_hunk={header}")
        fail = 1
    print(f"fmt_baseline_files={len(files)}")
    print(f"fmt_baseline_hunks={len(baseline)}")
    print(f"fmt_observed_hunks={len(observed)}")
    print(f"fmt_cargo_status={fmt_status}")
    if fmt_status != 0 and len(observed) == 0:
        print(f"fmt_gate=fail reason=cargo_fmt_check_exit={fmt_status} hunks=0")
        return 1
    if fail:
        print("fmt_gate=fail")
        return 1
    print("fmt_gate=pass")
    return 0


def main() -> None:
    if len(sys.argv) < 2:
        raise SystemExit(
            "usage: fmt-hunks.py tokens <root> <fmt.log>\n"
            "       fmt-hunks.py gate <root> <fmt.log> <files> <hunks> <fmt_status>"
        )
    command = sys.argv[1]
    if command == "tokens":
        root = Path(sys.argv[2])
        log_path = Path(sys.argv[3])
        for rel, lineno, digest in parse_hunks(
            log_path.read_text(encoding="utf-8", errors="replace"), root
        ):
            print(token(rel, lineno, digest))
        return
    if command == "gate":
        root = Path(sys.argv[2])
        log_path = Path(sys.argv[3])
        files_path = Path(sys.argv[4])
        hunks_path = Path(sys.argv[5])
        fmt_status = int(sys.argv[6])
        raise SystemExit(gate(root, log_path, files_path, hunks_path, fmt_status))
    raise SystemExit(f"unknown command {command}")


if __name__ == "__main__":
    main()
