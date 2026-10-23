#!/usr/bin/env python3
"""Display-text check beside the kind corpus compare.

Kind-equal lines whose display text differs are counted here. They are not
folded into the kind-based `worse` number from r4_compare.py / r6_compare.py.

Alignment matches the kind scripts: `--by id` (r4, last copy of a repeated
id wins) or `--by index` (r6, zip of the dump order).
"""

from __future__ import annotations

import argparse
import json
from collections import Counter


def load_id(path: str) -> dict:
    rows = {}
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if line.strip():
                doc = json.loads(line)
                rows[doc["id"]] = doc
    return rows


def load_index(path: str) -> list:
    rows = []
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if line.strip():
                rows.append(json.loads(line))
    return rows


def classify(base_display: str, tip_display: str) -> str:
    if base_display.replace("\r", "") == tip_display.replace("\r", ""):
        return "cr-only"
    if base_display.startswith(tip_display):
        suffix = base_display[len(tip_display) :]
        if suffix and set(suffix) <= set(" #") and "#" in suffix:
            return "closer-hide"
    return "other"


def pairs(tip_path: str, base_path: str, by: str):
    if by == "id":
        tip = load_id(tip_path)
        base = load_id(base_path)
        shared = [(tip[key], base[key]) for key in tip.keys() & base.keys()]
        return shared
    tip = load_index(tip_path)
    base = load_index(base_path)
    return list(zip(tip, base))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("tip")
    parser.add_argument("base")
    parser.add_argument("--by", choices=("id", "index"), required=True)
    parser.add_argument(
        "--prev",
        help="Earlier tip dump. Reports display bytes that moved relative to it.",
    )
    args = parser.parse_args()

    prev = None
    if args.prev:
        prev = load_id(args.prev) if args.by == "id" else load_index(args.prev)

    buckets = Counter()
    examples = {}
    moved = []
    kind_equal = 0
    for index, (tdoc, bdoc) in enumerate(pairs(args.tip, args.base, args.by)):
        tlines = tdoc["lines"]
        blines = bdoc["lines"]
        prev_lines = None
        if prev is not None:
            pdoc = prev.get(tdoc["id"]) if args.by == "id" else (prev[index] if index < len(prev) else None)
            prev_lines = pdoc["lines"] if pdoc else None
        n = min(len(tlines), len(blines))
        for i in range(n):
            t = tlines[i]
            b = blines[i]
            if t["k"] != b["k"] or t["d"] == b["d"]:
                continue
            kind_equal += 1
            bucket = classify(b["d"], t["d"])
            buckets[bucket] += 1
            examples.setdefault(
                bucket,
                (tdoc["id"], i, b["k"], b["d"], t["d"], t.get("s", "")),
            )
            if prev_lines is not None and i < len(prev_lines) and prev_lines[i]["d"] != t["d"]:
                moved.append((tdoc["id"], i, b["d"], prev_lines[i]["d"], t["d"], bucket))

    print(f"display_kind_equal_diff={kind_equal}")
    for name, count in buckets.most_common():
        print(f"display_bucket {name} {count}")
    print("display_examples")
    for name, (ident, i, kind, bd, td, src) in examples.items():
        print(f"  {name} {ident} L{i} kind={kind} base={bd!r} tip={td!r} src={src!r}")
    print(f"display_moved_vs_prev={len(moved)}")
    moved_buckets = Counter(row[5] for row in moved)
    for name, count in moved_buckets.most_common():
        print(f"display_moved_bucket {name} {count}")
    for row in moved[:40]:
        ident, i, bd, old, new, bucket = row
        print(f"  MOVED {bucket} {ident} L{i} base={bd!r} prev={old!r} tip={new!r}")


if __name__ == "__main__":
    main()
