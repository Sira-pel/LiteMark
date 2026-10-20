#!/usr/bin/env python3
"""Apply one reversible renderer mutant used by prove-red-mutants.sh.

Each mutant is a unique source replacement. The caller restores the file.
"""

from __future__ import annotations

import sys
from pathlib import Path

MUTANTS: dict[str, dict[str, str]] = {
    "headings-missing-marker": {
        "file": "src/markdown/parser.rs",
        "label": "parse_document default kind Paragraph to Blockquote",
        "old": "        let mut kind = LineKind::Paragraph;\n",
        "new": "        let mut kind = LineKind::Blockquote;\n",
    },
    "emphasis-no-markers": {
        "file": "src/markdown/parser.rs",
        "label": "Text segment bold forced true",
        "old": """                    styled_segments.push(StyledSegment {
                        source: abs_start..abs_end,
                        bold: !bold_stack.is_empty(),
                        italic: !italic_stack.is_empty(),
                        underline: !underline_stack.is_empty(),
                        strike: !strike_stack.is_empty(),
                        code: false,
                        link_url: link_stack.last().map(|(_, u, _)| u.clone()),
                    });
""",
        "new": """                    styled_segments.push(StyledSegment {
                        source: abs_start..abs_end,
                        bold: true,
                        italic: !italic_stack.is_empty(),
                        underline: !underline_stack.is_empty(),
                        strike: !strike_stack.is_empty(),
                        code: false,
                        link_url: link_stack.last().map(|(_, u, _)| u.clone()),
                    });
""",
    },
    "nested-lists-flat": {
        "file": "src/markdown/parser.rs",
        "label": "unordered list indent_cols + 7",
        "old": """            } else if trimmed_start.starts_with("- ")
                || trimmed_start.starts_with("* ")
                || trimmed_start.starts_with("+ ")
            {
                kind = LineKind::ListItem {
                    ordered: false,
                    number: 0,
                    task: None,
                    indent_cols: indent_len,
""",
        "new": """            } else if trimmed_start.starts_with("- ")
                || trimmed_start.starts_with("* ")
                || trimmed_start.starts_with("+ ")
            {
                kind = LineKind::ListItem {
                    ordered: false,
                    number: 0,
                    task: None,
                    indent_cols: indent_len + 7,
""",
    },
    "tables-not-a-table": {
        "file": "src/markdown/parser.rs",
        "label": "paragraph display_text gains !",
        "old": """        let (display_text, offset_map) =
            RowOffsetMap::from_hidden(line, line_start, body_base, &merged_hidden);
""",
        "new": """        let (mut display_text, offset_map) =
            RowOffsetMap::from_hidden(line, line_start, body_base, &merged_hidden);
        if matches!(kind, LineKind::Paragraph) {
            display_text.push('!');
        }
""",
    },
    "fenced-code-no-fence": {
        "file": "src/markdown/parser.rs",
        "label": "fence detector also matches line rust",
        "old": """        // Check code block fences
        if trimmed.starts_with("```") {
""",
        "new": """        // Check code block fences
        if trimmed.starts_with("```") || trimmed == "rust" {
""",
    },
    "links-plain": {
        "file": "src/markdown/parser.rs",
        "label": "Text segment link_url forced to mutant://link",
        "old": """                        code: false,
                        link_url: link_stack.last().map(|(_, u, _)| u.clone()),
""",
        "new": """                        code: false,
                        link_url: Some("mutant://link".to_string()),
""",
    },
    "images-markup": {
        "file": "src/markdown/parser.rs",
        "label": "display starting with ![ replaced by ALT",
        "old": """        let (display_text, offset_map) =
            RowOffsetMap::from_hidden(line, line_start, body_base, &merged_hidden);
""",
        "new": """        let (mut display_text, offset_map) =
            RowOffsetMap::from_hidden(line, line_start, body_base, &merged_hidden);
        if display_text.starts_with("![") {
            display_text = "ALT".to_string();
        }
""",
    },
    "long-lines-hard-wrap": {
        "file": "src/model/text_ops.rs",
        "label": "visual_rows skips wrapping",
        "old": "    if chars.len() <= wrap_cols {\n",
        "new": "    if chars.len() <= wrap_cols || chars.len() > 0 {\n",
    },
    "trailing-empty": {
        "file": "src/markdown/parser.rs",
        "label": "trailing newline does not append an empty line",
        "old": "        if markdown.ends_with('\\n') {\n",
        "new": "        if false && markdown.ends_with('\\n') {\n",
    },
}


def main() -> None:
    if len(sys.argv) != 4 or sys.argv[1] not in {"apply", "label"}:
        raise SystemExit(f"usage: {sys.argv[0]} apply|label <fixture> <root>")
    command, name, root = sys.argv[1:]
    spec = MUTANTS[name]
    if command == "label":
        print(spec["label"])
        print(spec["file"])
        return
    path = Path(root) / spec["file"]
    text = path.read_text(encoding="utf-8")
    count = text.count(spec["old"])
    if count != 1:
        raise SystemExit(f"mutant anchor count={count} fixture={name}")
    path.write_text(text.replace(spec["old"], spec["new"], 1), encoding="utf-8")


if __name__ == "__main__":
    main()
