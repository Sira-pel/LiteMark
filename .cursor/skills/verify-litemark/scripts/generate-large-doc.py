#!/usr/bin/env python3
"""Expand fixtures/large-doc/seed.txt into a deterministic markdown document.

The seed lists `sections=<n>` and templates `1=` ... `5=`. `{n}` is replaced
with the section number. Every template line ends with one `\n`, including
the empty template. No other bytes are added.
"""

from __future__ import annotations

import hashlib
import sys
from pathlib import Path


def load_seed(path: Path) -> tuple[int, list[str]]:
    sections: int | None = None
    templates: dict[int, str] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        if raw.startswith("sections="):
            sections = int(raw.split("=", 1)[1])
            continue
        key, value = raw.split("=", 1)
        templates[int(key)] = value
    if sections is None:
        raise SystemExit(f"missing sections= in {path}")
    ordered = [templates[index] for index in range(1, 6)]
    return sections, ordered


def render(sections: int, templates: list[str]) -> str:
    parts: list[str] = []
    for number in range(1, sections + 1):
        token = str(number)
        for template in templates:
            parts.append(template.replace("{n}", token))
            parts.append("\n")
    return "".join(parts)


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit(f"usage: {sys.argv[0]} <seed.txt> <output.md>")
    seed_path = Path(sys.argv[1])
    output_path = Path(sys.argv[2])
    sections, templates = load_seed(seed_path)
    text = render(sections, templates)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_bytes(text.encode("utf-8"))
    digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
    print(f"sha256={digest}")
    print(f"bytes={len(text.encode('utf-8'))}")


if __name__ == "__main__":
    main()
