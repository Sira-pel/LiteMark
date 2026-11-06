//! Inline format toggles (bold, italic, code, links) on the selection or caret.

use super::*;

impl DocumentTab {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn apply_format(&mut self, cmd: &str) {
        self.apply_format_in_mode(cmd, ViewMode::Raw);
    }

    pub fn apply_format_in_mode(&mut self, cmd: &str, mode: ViewMode) {
        if self.is_read_only() || !self.file_type.supports_markdown_features() {
            return;
        }
        // A format toggle is an undo boundary: pending span typing groups end
        // here so text typed after the toggle starts a new step.
        self.history.seal();

        // 1. Block-level formats: h1, h2, h3, ul, ol, checklist, quote
        let block_target = match cmd {
            "h1" => Some(BlockFormatTarget::Heading(1)),
            "h2" => Some(BlockFormatTarget::Heading(2)),
            "h3" => Some(BlockFormatTarget::Heading(3)),
            "h4" => Some(BlockFormatTarget::Heading(4)),
            "h5" => Some(BlockFormatTarget::Heading(5)),
            "h6" => Some(BlockFormatTarget::Heading(6)),
            "ul" => Some(BlockFormatTarget::BulletList),
            "ol" => Some(BlockFormatTarget::OrderedList),
            "checklist" => Some(BlockFormatTarget::TaskList),
            "quote" => Some(BlockFormatTarget::Blockquote),
            _ => None,
        };
        if let Some(target) = block_target {
            self.toggle_block_lines(target);
            return;
        }

        // 2. Code block toggle (Ctrl+Shift+C)
        if cmd == "codeblock" {
            self.toggle_code_block();
            return;
        }

        // 3. Inline styling & Link: bold, italic, underline, strike, code, link
        let inline_spec = match cmd {
            "bold" => Some((InlineTagKind::Bold, "**", "**", "bold text")),
            "italic" => Some((InlineTagKind::Italic, "*", "*", "italic text")),
            "underline" => Some((InlineTagKind::Underline, "<u>", "</u>", "underlined")),
            "strike" => Some((InlineTagKind::Strike, "~~", "~~", "strikethrough")),
            "code" => Some((InlineTagKind::Code, "`", "`", "code")),
            "link" => Some((InlineTagKind::Link, "[", "](https://)", "link")),
            _ => None,
        };

        if let Some((tag_kind, prefix, suffix, placeholder)) = inline_spec {
            let mut range = self.selection_range();
            let parsed = self.parsed();

            // If selection is empty, check if cursor is on/inside a word or enclosed by raw delimiters
            if range.is_empty() {
                let cursor = range.start;

                let word_start = crate::model::text_ops::word_left(&self.content, cursor);
                let word_end = crate::model::text_ops::word_right(&self.content, cursor);
                let line_s = home_offset(&self.content, cursor);
                let line_e = parsed
                    .line_at_offset(cursor)
                    .map(|line| visible_line_end(line, self.view_mode))
                    .unwrap_or_else(|| line_end(&self.content, cursor));
                let w_start = word_start.max(line_s);
                let w_end = word_end.min(line_e);
                let touching_word = cursor > w_start
                    && cursor < w_end
                    && self.content[..cursor]
                        .chars()
                        .next_back()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_')
                    && self.content[cursor..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphanumeric() || c == '_');
                if touching_word && w_start < w_end {
                    let extent = crate::model::text_ops::word_extent(&self.content, cursor);
                    if self.content[extent.clone()]
                        .chars()
                        .any(|c| c.is_alphanumeric() || c == '_')
                    {
                        range = extent;
                    }
                }

                if range.is_empty() {
                    // Check if cursor is strictly inside an existing InlinePair of `tag_kind`
                    if let Some(pair) = line_pair_refs(&parsed, range.start)
                        .iter()
                        .rev()
                        .find(|p| {
                            p.kind == tag_kind
                                && range.start > p.open_delim.start
                                && range.end < span_close_end(&parsed, p)
                        })
                        .cloned()
                    {
                        let inner_text = self.content[pair.inner.clone()].to_string();
                        let edit_range = pair.open_delim.start..pair.close_delim.end;
                        // If the inner text is just the default unmodified placeholder and the user immediately toggled off,
                        // remove the placeholder too so pressing Ctrl+B twice on an empty spot leaves it empty!
                        if inner_text == placeholder && range == pair.inner {
                            let pos = pair.open_delim.start;
                            self.edit(edit_range, "");
                            self.caret.move_to(pos);
                            self.pending_formats = ActiveFormats::default();
                            self.refresh_cursor();
                            return;
                        }
                        if mode == ViewMode::Wysiwyg && tag_kind != InlineTagKind::Link {
                            // Word-style exit: the span stays formatted and the
                            // caret moves past the whole closing run so further
                            // typing is plain in the toggled format. Other
                            // formats stacked around the caret stay armed for
                            // the next letter, and a second press of the same
                            // toggle then disarms them. Links unwrap instead.
                            let mut pending = pending_formats_at(&parsed, range.start);
                            match tag_kind {
                                InlineTagKind::Bold => pending.bold = false,
                                InlineTagKind::Italic => pending.italic = false,
                                InlineTagKind::Underline => pending.underline = false,
                                InlineTagKind::Strike => pending.strike = false,
                                InlineTagKind::Code => pending.code = false,
                                InlineTagKind::Link => {}
                            }
                            if range.start > pair.open_delim.start {
                                let group_end = span_close_end(&parsed, &pair);
                                self.caret.move_to(floor_boundary(&self.content, group_end));
                            }
                            self.pending_formats = pending;
                            self.refresh_cursor();
                            return;
                        }
                        let new_start = pair.open_delim.start;
                        self.edit(edit_range, &inner_text);
                        let rel = range
                            .start
                            .saturating_sub(pair.inner.start)
                            .min(inner_text.len());
                        self.caret.move_to(new_start + rel);
                        self.pending_formats = ActiveFormats::default();
                        self.refresh_cursor();
                        return;
                    }

                    // Check empty delimiters right around cursor, e.g. `**|**`
                    if outer_is_delimited(&self.content, cursor, cursor, prefix, suffix) {
                        let edit_range = (cursor - prefix.len())..(cursor + suffix.len());
                        let pos = cursor - prefix.len();
                        self.edit(edit_range, "");
                        self.caret.move_to(pos);
                        self.pending_formats = ActiveFormats::default();
                        self.refresh_cursor();
                        return;
                    }

                    if outer_is_delimited(&self.content, word_start, word_end, prefix, suffix) {
                        let core = self.content[word_start..word_end].to_string();
                        let edit_range = (word_start - prefix.len())..(word_end + suffix.len());
                        let new_start = word_start - prefix.len();
                        let new_end = new_start + core.len();
                        self.edit(edit_range, &core);
                        self.caret.range = new_start..new_end;
                        self.caret.reversed = false;
                        self.pending_formats = ActiveFormats::default();
                        self.refresh_cursor();
                        return;
                    }
                }
            }

            if range.is_empty() {
                // Caret on the opener: unwrap the span (Left to offset 0 of
                // `**Hello**` then Ctrl+B must drop the markers).
                if let Some(pair) = line_pair_refs(&parsed, range.start)
                    .iter()
                    .rev()
                    .find(|p| p.kind == tag_kind && range.start == p.open_delim.start)
                    .cloned()
                {
                    let inner_text = self.content[pair.inner.clone()].to_string();
                    let edit_range = pair.open_delim.start..pair.close_delim.end;
                    self.edit(edit_range, &inner_text);
                    self.caret.move_to(pair.open_delim.start);
                    self.pending_formats = ActiveFormats::default();
                    self.refresh_cursor();
                    return;
                }

                // In WYSIWYG mode, toggling bold/italic/underline/strike/code at an empty cursor toggles pending_formats!
                if mode == ViewMode::Wysiwyg && tag_kind != InlineTagKind::Link {
                    match tag_kind {
                        InlineTagKind::Bold => {
                            self.pending_formats.bold = !self.pending_formats.bold
                        }
                        InlineTagKind::Italic => {
                            self.pending_formats.italic = !self.pending_formats.italic
                        }
                        InlineTagKind::Underline => {
                            self.pending_formats.underline = !self.pending_formats.underline
                        }
                        InlineTagKind::Strike => {
                            self.pending_formats.strike = !self.pending_formats.strike
                        }
                        InlineTagKind::Code => {
                            self.pending_formats.code = !self.pending_formats.code
                        }
                        InlineTagKind::Link => {}
                    }
                    self.refresh_cursor();
                    return;
                }

                // In Raw mode (or for Link), insert placeholder and select inner placeholder so a second press unwraps/removes it
                self.edit(
                    range.clone(),
                    &format!("{}{}{}", prefix, placeholder, suffix),
                );
                let start = floor_boundary(&self.content, range.start + prefix.len());
                let end = floor_boundary(&self.content, start + placeholder.len());
                self.caret.range = start..end;
                self.caret.reversed = false;
                self.refresh_cursor();
                return;
            }

            // Non-empty selection: collect parts across table cells or lines separately
            let orig_cursor = if self.caret.range.is_empty() {
                Some(self.caret.range.start)
            } else {
                None
            };
            let orig_sel = range.clone();
            struct InlinePartInfo {
                range: Range<usize>,
                has_format: bool,
                edit_range: Range<usize>,
                unwrapped_text: String,
                wrapped_text: String,
                /// For a partial unformat, the new range of the selection's
                /// text after the surrounding span was split.
                split_inner: Option<Range<usize>>,
            }

            let mut candidate_ranges: Vec<Range<usize>> = Vec::new();

            for pline in parsed.lines_overlapping(range.clone()) {
                let line_start = range.start.max(pline.source.start);
                let line_end = range.end.min(pline.source.end);
                if line_start >= line_end {
                    continue;
                }
                match &pline.kind {
                    // Code block bodies show markers literally, so a select-all
                    // toggle leaves them alone like the fences around them.
                    LineKind::TableDelimiter
                    | LineKind::CodeFence { .. }
                    | LineKind::CodeLine
                    | LineKind::HorizontalRule => {
                        continue;
                    }
                    LineKind::TableRow { ref body, .. } => {
                        let TableRowBody {
                            cells, raw_cells, ..
                        } = &**body;
                        let cell_ranges = if raw_cells.is_empty() {
                            cells
                        } else {
                            raw_cells
                        };
                        for c_range in cell_ranges {
                            let c_start = range.start.max(c_range.start);
                            let c_end = range.end.min(c_range.end);
                            if c_start >= c_end {
                                continue;
                            }
                            let slice = &self.content[c_start..c_end];
                            let trimmed = slice.trim();
                            if trimmed.is_empty() {
                                continue;
                            }
                            let lead = slice.len() - slice.trim_start().len();
                            let trail = slice.len() - slice.trim_end().len();
                            let part_start = c_start + lead;
                            let part_end = c_end - trail;
                            candidate_ranges.push(part_start..part_end);
                        }
                    }
                    _ => {
                        let content_start = pline.prefix_range.end.max(pline.source.start);
                        // A heading's closing-hash run (`# Title #`) is hidden
                        // syntax; wrapping it would make the `#` visible text.
                        let content_end = match pline.kind {
                            LineKind::Heading { .. } => {
                                let text = self.content[pline.source.clone()]
                                    .trim_end_matches(['\n', '\r']);
                                crate::markdown::parser::atx_content_range(text)
                                    .map_or(pline.source.end, |c| pline.source.start + c.end)
                            }
                            _ => pline.source.end,
                        };
                        let l_start = range.start.max(content_start);
                        let l_end = range.end.min(content_end);
                        if l_start >= l_end {
                            continue;
                        }
                        let slice = &self.content[l_start..l_end];
                        let trimmed = slice.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        let lead = slice.len() - slice.trim_start().len();
                        let trail = slice.len() - slice.trim_end().len();
                        let part_start = l_start + lead;
                        let part_end = l_end - trail;
                        candidate_ranges.push(part_start..part_end);
                    }
                }
            }

            if candidate_ranges.is_empty() {
                return;
            }

            if tag_kind != InlineTagKind::Link {
                // Rewrite each line (or table cell) on its own. Inline pairs never
                // cross lines, and one span over several lines would put markers
                // before list, task and quote prefixes and across paragraphs.
                let rewrite_ranges: Vec<Range<usize>> = candidate_ranges.clone();
                let rewrite = |on: bool| -> Vec<Option<InlineRewrite>> {
                    rewrite_ranges
                        .iter()
                        .map(|p_range| {
                            let p_start = p_range.start;
                            let p_end = p_range.end;
                            let pline = match parsed.line_at_offset(p_start) {
                                Some(l) => l,
                                None => return None,
                            };

                            let mut group_pairs = collect_group_pairs(&parsed, p_start..p_end);

                            let (replace_start, replace_end) = if group_pairs.is_empty() {
                                (p_start, p_end)
                            } else {
                                let r_start = group_pairs
                                    .iter()
                                    .map(|p| p.open_delim.start)
                                    .min()
                                    .unwrap()
                                    .min(p_start);
                                let r_end = group_pairs
                                    .iter()
                                    .map(|p| span_close_end(&parsed, p))
                                    .max()
                                    .unwrap()
                                    .max(p_end);
                                (r_start, r_end)
                            };
                            let (replace_start, replace_end) = expand_replace_to_matching_neighbors(
                                &self.content,
                                &parsed,
                                pline,
                                p_start..p_end,
                                replace_start..replace_end,
                                tag_kind,
                                on,
                            );
                            // Neighbor expansion may pull in spans that did not overlap
                            // the selection (`aa` next to `**bb cc**`). Skip those
                            // delimiters so they are rewritten instead of kept as text.
                            add_contained_pairs(
                                &parsed,
                                replace_start..replace_end,
                                &mut group_pairs,
                            );

                            let simple_wrap = group_pairs.is_empty()
                                && replace_start == p_start
                                && replace_end == p_end;
                            let slices: Vec<StyledSlice> = if simple_wrap {
                                let mut flags = FormatFlags::default();
                                apply_toggle(&mut flags, tag_kind, on);
                                vec![StyledSlice {
                                    text: self.content[p_start..p_end].to_string(),
                                    flags,
                                    is_selected: true,
                                }]
                            } else {
                                let skip: Vec<Range<usize>> = group_pairs
                                    .iter()
                                    .flat_map(|p| [p.open_delim.clone(), p.close_delim.clone()])
                                    .collect();
                                let segs: Vec<&StyledSegment> = parsed
                                    .lines_overlapping(replace_start..replace_end)
                                    .iter()
                                    .flat_map(|l| l.styled_segments.iter())
                                    .collect();
                                let local_pairs =
                                    nearby_pair_refs(&parsed, replace_start..replace_end);
                                collect_rewrite_slices(
                                    &self.content,
                                    &local_pairs,
                                    &segs,
                                    replace_start..replace_end,
                                    p_start..p_end,
                                    &skip,
                                    Some((tag_kind, on)),
                                )
                            };
                            let glued_left = replace_start > 0
                                && self.content[..replace_start]
                                    .chars()
                                    .next_back()
                                    .is_some_and(|c| !c.is_whitespace());
                            // Each line is rewritten on its own, so ask the whole
                            // selection whether it crosses a line break.
                            let include_wraps = self.content[orig_sel.clone()].contains('\n');
                            let (rendered_str, sel_in_rendered) =
                                render_styled_slices_with_selection_opts(
                                    &slices,
                                    glued_left,
                                    include_wraps,
                                );
                            Some((
                                replace_start..replace_end,
                                rendered_str,
                                sel_in_rendered,
                                p_start,
                            ))
                        })
                        .collect()
                };

                // Lines the parser cannot show in this format (a `~~~` fence it
                // reads as text, an indented line that drops `<u>`) are left
                // alone. Writing markers there would not render, and the next
                // press would wrap them again instead of toggling off.
                let trial_on = rewrite(true);
                let can_format =
                    verify_inline_rewrites(&self.content, &parsed, &trial_on, tag_kind, true);
                if !can_format.iter().any(|ok| *ok) {
                    return;
                }

                // Whitespace between two spans (`**a** **b**`) is unformatted
                // but invisible, so it must not make a second press re-apply
                // the format instead of removing it.
                let global_all_have_format = rewrite_ranges
                    .iter()
                    .zip(&can_format)
                    .filter(|(_, ok)| **ok)
                    .all(|(p_range, _)| {
                        let Some(pline) = parsed.line_at_offset(p_range.start) else {
                            return false;
                        };
                        let mut segs = pline.styled_segments.iter().filter(|s| {
                            s.source.start < p_range.end
                                && s.source.end > p_range.start
                                && !self.content[s.source.clone()].trim().is_empty()
                        });
                        let mut any = false;
                        segs.all(|s| {
                            any = true;
                            segment_flags(s).get(tag_kind)
                        }) && any
                    });
                let should_turn_on = self.force_inline_toggle.unwrap_or(!global_all_have_format);

                let (results, ok) = if should_turn_on {
                    (trial_on, can_format)
                } else {
                    let off = rewrite(false);
                    let ok = verify_inline_rewrites(&self.content, &parsed, &off, tag_kind, false);
                    (off, ok)
                };
                let mut candidate_results: Vec<InlineRewrite> = results
                    .into_iter()
                    .zip(ok)
                    .filter_map(|(result, ok)| if ok { result } else { None })
                    .collect();

                if rewrite_ranges.len() == 1 && !candidate_results.is_empty() {
                    let (rep_range, rendered_str, sel_in_rendered, p_start) = candidate_results.remove(0);
                    self.edit(rep_range.clone(), &rendered_str);

                    if let Some(cur) = orig_cursor {
                        let cursor_in_sel = cur.saturating_sub(p_start);
                        let target_pos = rep_range.start + sel_in_rendered.start + cursor_in_sel.min(sel_in_rendered.len());
                        self.caret.move_to(floor_boundary(&self.content, target_pos));
                    } else {
                        let new_start = rep_range.start + sel_in_rendered.start;
                        let new_end = rep_range.start + sel_in_rendered.end;
                        self.caret.range = new_start..new_end;
                        self.caret.reversed = false;
                    }
                    self.pending_formats = ActiveFormats::default();
                    self.refresh_cursor();
                    return;
                }

                let edits: Vec<(Range<usize>, String, Range<usize>)> = candidate_results
                    .into_iter()
                    .filter(|(r, repl, _, _)| self.content[r.clone()] != *repl)
                    .map(|(r, repl, sel, _)| (r, repl, sel))
                    .collect();

                if edits.is_empty() {
                    return;
                }

                let min_start = edits.first().unwrap().0.start;
                let max_end = edits.last().unwrap().0.end;
                let span_range = min_start..max_end;

                let mut replacement = String::with_capacity(
                    max_end - min_start + edits.len() * (prefix.len() + suffix.len()),
                );
                let mut cursor = min_start;
                for (r, repl, _) in &edits {
                    if r.start > cursor {
                        replacement.push_str(&self.content[cursor..r.start]);
                    }
                    replacement.push_str(repl);
                    cursor = r.end;
                }
                if max_end > cursor {
                    replacement.push_str(&self.content[cursor..max_end]);
                }

                self.edit(span_range, &replacement);

                // A selection edge inside a rewritten span lands on the selected
                // text's edge in the replacement, not at the end of the span.
                let map_offset = |offset: usize, is_start: bool| -> usize {
                    if offset < min_start {
                        return offset;
                    }
                    let mut mapped = min_start;
                    let mut orig_cursor = min_start;
                    for (r, repl, sel) in &edits {
                        if offset < r.start {
                            mapped += offset - orig_cursor;
                            return mapped;
                        }
                        mapped += r.start - orig_cursor;
                        if offset <= r.end {
                            mapped += if is_start { sel.start } else { sel.end };
                            return mapped;
                        }
                        mapped += repl.len();
                        orig_cursor = r.end;
                    }
                    if offset > orig_cursor {
                        mapped += offset - orig_cursor;
                    }
                    mapped
                };
                let new_start = map_offset(orig_sel.start, true);
                let new_end = map_offset(orig_sel.end, false);
                let new_start = floor_boundary(&self.content, new_start);
                let new_end = floor_boundary(&self.content, new_end);
                self.caret.range = new_start..new_end;
                self.caret.reversed = false;
                self.pending_formats = ActiveFormats::default();
                self.refresh_cursor();
                return;
            }

            let mut parts: Vec<InlinePartInfo> = Vec::with_capacity(candidate_ranges.len());

            for p_range in candidate_ranges {
                let p_start = p_range.start;
                let p_end = p_range.end;
                let part_text = &self.content[p_start..p_end];

                let (has_format, edit_range, unwrapped_text, split_inner) = if tag_kind
                    == InlineTagKind::Link
                {
                    if let Some(inner) = is_single_link(part_text) {
                        (true, p_start..p_end, inner.to_string(), None)
                    } else if let Some(pair) = line_pair_refs(&parsed, p_start).iter().find(|p| {
                        p.kind == InlineTagKind::Link
                            && (p.inner == (p_start..p_end)
                                || (p.open_delim.start == p_start && p.close_delim.end == p_end))
                    }) {
                        (
                            true,
                            pair.open_delim.start..pair.close_delim.end,
                            self.content[pair.inner.clone()].to_string(),
                            None,
                        )
                    } else if let Some((outer_range, inner)) =
                        unwrap_outer_link(&self.content, p_start, p_end)
                    {
                        (true, outer_range, inner, None)
                    } else {
                        (false, p_start..p_end, String::new(), None)
                    }
                } else {
                    (false, p_start..p_end, String::new(), None)
                };

                let wrapped_text = format!("{}{}{}", prefix, part_text, suffix);
                parts.push(InlinePartInfo {
                    range: p_range,
                    has_format,
                    edit_range,
                    unwrapped_text,
                    wrapped_text,
                    split_inner,
                });
            }

            let every_part_has_format = parts.iter().all(|p| p.has_format);
            let edits: Vec<(Range<usize>, String)> = if every_part_has_format {
                parts
                    .iter()
                    .map(|p| (p.edit_range.clone(), p.unwrapped_text.clone()))
                    .collect()
            } else {
                parts
                    .iter()
                    .filter(|p| !p.has_format)
                    .map(|p| (p.edit_range.clone(), p.wrapped_text.clone()))
                    .collect()
            };

            if edits.is_empty() {
                return;
            }

            let min_start = edits.first().unwrap().0.start;
            let max_end = edits.last().unwrap().0.end;
            let span_range = min_start..max_end;

            let mut replacement = String::with_capacity(
                max_end - min_start + edits.len() * (prefix.len() + suffix.len()),
            );
            let mut cursor = min_start;
            for (r, repl) in &edits {
                if r.start > cursor {
                    replacement.push_str(&self.content[cursor..r.start]);
                }
                replacement.push_str(repl);
                cursor = r.end;
            }
            if max_end > cursor {
                replacement.push_str(&self.content[cursor..max_end]);
            }

            self.edit(span_range, &replacement);

            if parts.len() == 1
                && (orig_sel == (parts[0].range.start..parts[0].range.end)
                    || orig_sel == (parts[0].edit_range.start..parts[0].edit_range.end))
            {
                if let (Some(cur), true) = (orig_cursor, every_part_has_format) {
                    let unwrapped_len = edits[0].1.len();
                    let rel = cur
                        .saturating_sub(parts[0].range.start)
                        .min(unwrapped_len);
                    self.caret.move_to(min_start + rel);
                    self.caret.reversed = false;
                    self.pending_formats = ActiveFormats::default();
                    self.refresh_cursor();
                    return;
                }
                if let Some(split) = parts[0].split_inner.clone() {
                    self.caret.range = split;
                } else if every_part_has_format {
                    let unwrapped_len = edits[0].1.len();
                    self.caret.range = min_start..min_start + unwrapped_len;
                } else {
                    let inner_start = min_start + prefix.len();
                    let inner_end = inner_start + (parts[0].range.end - parts[0].range.start);
                    self.caret.range = inner_start..inner_end;
                }
                self.caret.reversed = false;
            } else {
                let map_offset = |offset: usize| -> usize {
                    if offset <= min_start {
                        return offset;
                    }
                    let mut mapped = min_start;
                    let mut orig_cursor = min_start;
                    for (r, repl) in &edits {
                        if offset <= r.start {
                            mapped += offset - orig_cursor;
                            return mapped;
                        }
                        mapped += r.start - orig_cursor;
                        if offset <= r.end {
                            mapped += repl.len();
                            return mapped;
                        }
                        mapped += repl.len();
                        orig_cursor = r.end;
                    }
                    if offset > orig_cursor {
                        mapped += offset - orig_cursor;
                    }
                    mapped
                };
                let new_start = map_offset(orig_sel.start);
                let new_end = map_offset(orig_sel.end);
                let new_start = floor_boundary(&self.content, new_start);
                let new_end = floor_boundary(&self.content, new_end).max(new_start);
                self.caret.range = new_start..new_end;
                self.caret.reversed = false;
            }
            self.pending_formats = ActiveFormats::default();
            self.refresh_cursor();
            return;
        }

        if cmd == "table" {
            let range = self.selection_range();
            let parsed = self.parsed();

            // Check if cursor is currently inside an existing table
            let in_table = parsed.lines.iter().find(|l| {
                if let LineKind::TableRow { ref body, .. } = l.kind {
                    let TableRowBody { table_source, .. } = &**body;
                    range.start >= table_source.start && range.start <= table_source.end
                } else {
                    false
                }
            });

            if let Some(table_line) = in_table {
                if let LineKind::TableRow { ref body, .. } = table_line.kind {
                    let TableRowBody {
                        cells,
                        table_source,
                        ..
                    } = &**body;
                    let col_count = cells.len().max(2);
                    let row_str = format!("\n|{}", "  |".repeat(col_count));
                    let insert_pos = table_source.end;
                    self.edit(insert_pos..insert_pos, &row_str);
                    let target_pos = (insert_pos + 3).min(self.content.len());
                    self.caret
                        .move_to(floor_boundary(&self.content, target_pos));
                    self.refresh_cursor();
                    return;
                }
            }

            // Outside table: insert clean standard 2-column table template
            let mut prefix = String::new();
            if range.start > 0 {
                let prev_char = self.content[..range.start].chars().next_back();
                if let Some(c) = prev_char {
                    if c != '\n' {
                        prefix.push_str("\n\n");
                    } else if !self.content[..range.start].ends_with("\n\n") {
                        prefix.push('\n');
                    }
                }
            }
            let mut suffix = String::new();
            if range.end < self.content.len() {
                let next_char = self.content[range.end..].chars().next();
                if let Some(c) = next_char {
                    if c != '\n' {
                        suffix.push_str("\n\n");
                    } else if !self.content[range.end..].starts_with("\n\n") {
                        suffix.push('\n');
                    }
                }
            }
            let table_template = "| Column 1 | Column 2 |\n| :--- | :--- |\n| Item 1 | Item 2 |";
            let full_insert = format!("{}{}{}", prefix, table_template, suffix);
            self.edit(range.clone(), &full_insert);
            let col1_start = range.start + prefix.len() + 2;
            let col1_end = col1_start + "Column 1".len();
            self.caret.range = col1_start..col1_end;
            self.caret.reversed = false;
            self.refresh_cursor();
            return;
        }

        if cmd == "clear" {
            self.clear_inline_formats(mode);
            return;
        }

        // 4. Fallback for clear, hr, table
        let range = self.selection_range();
        let selected = self.content[range.clone()].to_string();
        let flags = line_start_flags(&self.content, range.start, &selected);
        let Some(replacement) = format_replacement(cmd, &selected, &flags) else {
            return;
        };
        let cursor_after_placeholder = cmd_selects_placeholder(cmd) && selected.is_empty();
        let prefix_len = placeholder_prefix_len(cmd);
        self.edit(range.clone(), &replacement);
        if cursor_after_placeholder {
            let start = floor_boundary(&self.content, range.start + prefix_len);
            let end = floor_boundary(&self.content, start + placeholder_len(cmd));
            self.caret.range = start..end;
            self.caret.reversed = false;
            self.refresh_cursor();
        }
    }

    /// True when any text in `range` carries the inline format `kind`.
    pub(super) fn range_has_format(&mut self, range: &Range<usize>, kind: InlineTagKind) -> bool {
        let parsed = self.parsed();
        parsed.lines_overlapping(range.clone()).iter().any(|line| {
            line.styled_segments.iter().any(|seg| {
                seg.source.start < range.end
                    && seg.source.end > range.start
                    && match kind {
                        InlineTagKind::Bold => seg.bold,
                        InlineTagKind::Italic => seg.italic,
                        InlineTagKind::Underline => seg.underline,
                        InlineTagKind::Strike => seg.strike,
                        InlineTagKind::Code => seg.code,
                        InlineTagKind::Link => false,
                    }
            })
        })
    }

    /// Clear Formatting: removes bold, italic, underline, strikethrough and
    /// code from the selection (or the word at the caret), splitting spans
    /// that stick out of it. The work happens on a scratch copy and lands as
    /// one edit, so a single undo restores everything.
    pub(super) fn clear_inline_formats(&mut self, mode: ViewMode) {
        let mut range = self.selection_range();
        if range.is_empty() {
            let extent = crate::model::text_ops::word_extent(&self.content, range.start);
            if extent.is_empty()
                || !self.content[extent.clone()]
                    .chars()
                    .any(|c| c.is_alphanumeric() || c == '_')
            {
                return;
            }
            range = extent;
        }
        let mut tmp = DocumentTab::new(
            "clear".into(),
            "clear.md".into(),
            self.content.clone(),
            None,
            None,
        );
        tmp.view_mode = mode;
        tmp.force_inline_toggle = Some(false);
        tmp.caret.range = range;

        let kinds = [
            (InlineTagKind::Bold, "bold"),
            (InlineTagKind::Italic, "italic"),
            (InlineTagKind::Underline, "underline"),
            (InlineTagKind::Strike, "strike"),
            (InlineTagKind::Code, "code"),
        ];
        for (kind, cmd) in kinds {
            let sel = tmp.selection_range();
            if tmp.range_has_format(&sel, kind) {
                tmp.apply_format_in_mode(cmd, mode);
            }
        }

        // Links and any markers wholly inside the selection.
        let sel = tmp.selection_range();
        let selected = tmp.content[sel.clone()].to_string();
        let flags = line_start_flags(&tmp.content, sel.start, &selected);
        if let Some(replacement) = format_replacement("clear", &selected, &flags) {
            if replacement != selected {
                tmp.edit(sel.clone(), &replacement);
                tmp.caret.range = sel.start..sel.start + replacement.len();
            }
        }

        let (old, new) = (&self.content, &tmp.content);
        if old == new {
            return;
        }
        let mut prefix = old
            .bytes()
            .zip(new.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
            prefix -= 1;
        }
        let max_suffix = old.len().min(new.len()) - prefix;
        let mut suffix = old
            .bytes()
            .rev()
            .zip(new.bytes().rev())
            .take(max_suffix)
            .take_while(|(a, b)| a == b)
            .count();
        while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix)
        {
            suffix -= 1;
        }
        let replacement = new[prefix..new.len() - suffix].to_string();
        let target = tmp.selection_range();
        self.edit(prefix..old.len() - suffix, &replacement);
        let end = floor_boundary(&self.content, target.end);
        let start = floor_boundary(&self.content, target.start).min(end);
        self.caret.range = start..end;
        self.caret.reversed = false;
        self.pending_formats = ActiveFormats::default();
        self.refresh_cursor();
    }
}

/// True when `from..to` is only whitespace and delimiter markup, so a
/// neighboring span with matching flags can merge across that gap.
pub(super) fn gap_is_ws_and_markers(content: &str, hidden: &[Range<usize>], from: usize, to: usize) -> bool {
    if to < from {
        return false;
    }
    let mut i = from;
    while i < to {
        if content[i..].starts_with('\n') {
            return false;
        }
        if let Some(h) = hidden.iter().find(|r| i >= r.start && i < r.end) {
            if !is_delimiter_markup(&content[h.start..h.end]) {
                return false;
            }
            i = h.end;
            continue;
        }
        let Some(ch) = content[i..].chars().next() else {
            break;
        };
        if ch.is_whitespace() && ch != '\n' {
            i += ch.len_utf8();
            continue;
        }
        return false;
    }
    true
}

pub(super) fn is_delimiter_markup(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty()
        && (t.chars().all(|c| matches!(c, '*' | '_' | '~' | '`'))
            || t.eq_ignore_ascii_case("<u>")
            || t.eq_ignore_ascii_case("</u>"))
}

pub(super) fn covering_span(
    parsed: &ParsedDocument,
    pline: &ParsedLine,
    seg: &StyledSegment,
) -> (usize, usize) {
    let overlapping: Vec<&InlinePair> = pline
        .inline_pairs
        .iter()
        .filter(|p| {
            p.kind != InlineTagKind::Link
                && p.inner.start <= seg.source.start
                && p.inner.end >= seg.source.end
        })
        .collect();
    if overlapping.is_empty() {
        return (seg.source.start, seg.source.end);
    }
    let open = overlapping
        .iter()
        .map(|p| span_open_start(parsed, p))
        .min()
        .unwrap_or(seg.source.start);
    let close = overlapping
        .iter()
        .map(|p| span_close_end(parsed, p))
        .max()
        .unwrap_or(seg.source.end);
    (open, close)
}

/// When turning a format on, extend the rewrite so a neighboring span that
/// already has the same flags is merged instead of emitting `**aa**** bb**`.
pub(super) fn expand_replace_to_matching_neighbors(
    content: &str,
    parsed: &ParsedDocument,
    pline: &ParsedLine,
    p_range: Range<usize>,
    mut replace: Range<usize>,
    tag_kind: InlineTagKind,
    should_turn_on: bool,
) -> (usize, usize) {
    if !should_turn_on {
        return (replace.start, replace.end);
    }
    let p_start = p_range.start;
    let p_end = p_range.end;
    let overlapping: Vec<&StyledSegment> = pline
        .styled_segments
        .iter()
        .filter(|s| s.source.start < p_end && s.source.end > p_start)
        .collect();
    if overlapping
        .windows(2)
        .any(|w| segment_flags(w[0]) != segment_flags(w[1]))
    {
        return (replace.start, replace.end);
    }
    let mut target = overlapping
        .first()
        .map(|s| segment_flags(s))
        .unwrap_or_default();
    match tag_kind {
        InlineTagKind::Bold => target.bold = true,
        InlineTagKind::Italic => target.italic = true,
        InlineTagKind::Underline => target.underline = true,
        InlineTagKind::Strike => target.strike = true,
        InlineTagKind::Code => target.code = true,
        InlineTagKind::Link => return (replace.start, replace.end),
    }
    if !target.any_set() {
        return (replace.start, replace.end);
    }

    let mut left_front = p_start;
    let mut right_front = p_end;
    loop {
        let mut changed = false;
        for seg in &pline.styled_segments {
            if segment_flags(seg) != target {
                continue;
            }
            if seg.source.end <= left_front
                && gap_is_ws_and_markers(content, &pline.hidden_ranges, seg.source.end, left_front)
            {
                let (open, close) = covering_span(parsed, pline, seg);
                if open < replace.start || close > replace.end || seg.source.start < left_front {
                    replace.start = replace.start.min(open);
                    replace.end = replace.end.max(close);
                    left_front = left_front.min(seg.source.start);
                    changed = true;
                }
            }
            if seg.source.start >= right_front
                && gap_is_ws_and_markers(
                    content,
                    &pline.hidden_ranges,
                    right_front,
                    seg.source.start,
                )
            {
                let (open, close) = covering_span(parsed, pline, seg);
                if close > replace.end || open < replace.start || seg.source.end > right_front {
                    replace.start = replace.start.min(open);
                    replace.end = replace.end.max(close);
                    right_front = right_front.max(seg.source.end);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    (replace.start, replace.end)
}

/// The end of the whole closing-marker run of the stacked group `pair`
/// belongs to (the non-link pairs sharing its content range), so inserted
/// spaces and exits clear every stacked closer (`***`, `~~**`).
pub(super) fn span_close_end(_parsed: &ParsedDocument, pair: &InlinePair) -> usize {
    pair.group_close_end
}

/// The start of the whole opening-marker run of the stacked group `pair`
/// belongs to, so whole-span edits cover every stacked opener (`***`, `<u>**`).
pub(super) fn span_open_start(_parsed: &ParsedDocument, pair: &InlinePair) -> usize {
    pair.group_open_start
}

pub(super) fn nearby_pair_refs(parsed: &ParsedDocument, range: Range<usize>) -> Vec<&InlinePair> {
    parsed
        .lines_near(range)
        .iter()
        .flat_map(|l| l.inline_pairs.iter())
        .collect()
}

/// A code line that comes from indentation, not from inside a fence.
pub(super) fn is_indented_code(parsed: &ParsedDocument, line: &ParsedLine) -> bool {
    matches!(line.kind, LineKind::CodeLine)
        && !parsed
            .code_blocks
            .iter()
            .any(|(open, close)| line.source.start >= open.start && line.source.start <= close.end)
}

/// The indented code block around `offset`: its consecutive code lines plus
/// any run of blank lines between them (CommonMark example 117), or `None`
/// when `offset` is not on an indented code line.
pub(super) fn indented_code_block_at(parsed: &ParsedDocument, offset: usize) -> Option<&[ParsedLine]> {
    let at = parsed.line_at_offset(offset)?.line_index;
    let lines = &parsed.lines;
    let code = |index: usize| is_indented_code(parsed, &lines[index]);
    if !code(at) {
        return None;
    }
    let blank = |index: usize| matches!(lines[index].kind, LineKind::Empty);
    let mut first = at;
    let mut probe = at;
    while probe > 0 && (code(probe - 1) || blank(probe - 1)) {
        probe -= 1;
        if code(probe) {
            first = probe;
        }
    }
    let mut last = at;
    let mut probe = at;
    while probe + 1 < lines.len() && (code(probe + 1) || blank(probe + 1)) {
        probe += 1;
        if code(probe) {
            last = probe;
        }
    }
    Some(&lines[first..=last])
}

pub(super) fn line_pair_refs(parsed: &ParsedDocument, offset: usize) -> &[InlinePair] {
    parsed
        .line_at_offset(offset)
        .map(|l| l.inline_pairs.as_slice())
        .unwrap_or(&[])
}

pub(super) fn pair_id(p: &InlinePair) -> (usize, usize, InlineTagKind) {
    (p.open_delim.start, p.close_delim.end, p.kind)
}

/// Connected component of overlapping non-link pairs on one line. Pairs on
/// different lines have disjoint source ranges, so a document-wide expansion
/// is the union of per-line expansions.
pub(super) fn expand_line_group_pairs<'a>(
    line: &'a ParsedLine,
    parsed: &ParsedDocument,
    range: Range<usize>,
    group_pairs: &mut Vec<&'a InlinePair>,
    seen: &mut HashSet<(usize, usize, InlineTagKind)>,
) {
    let mut cands: Vec<&InlinePair> = line
        .inline_pairs
        .iter()
        .filter(|p| p.kind != InlineTagKind::Link)
        .collect();
    if cands.is_empty() {
        return;
    }
    cands.sort_by_key(|p| (p.open_delim.start, span_close_end(parsed, p)));
    let mut lo = range.start;
    let mut hi = range.end.max(range.start);
    loop {
        let lim = cands.partition_point(|p| p.open_delim.start < hi);
        let mut nlo = lo;
        let mut nhi = hi;
        for p in &cands[..lim] {
            let close = span_close_end(parsed, p);
            if close > nlo {
                nlo = nlo.min(p.open_delim.start);
                nhi = nhi.max(close);
            }
        }
        if nlo == lo && nhi == hi {
            break;
        }
        lo = nlo;
        hi = nhi;
    }
    group_pairs.reserve(cands.len());
    let lim = cands.partition_point(|p| p.open_delim.start < hi);
    for p in cands[..lim].iter().copied() {
        if span_close_end(parsed, p) > lo && seen.insert(pair_id(p)) {
            group_pairs.push(p);
        }
    }
}

pub(super) fn collect_group_pairs(parsed: &ParsedDocument, range: Range<usize>) -> Vec<&InlinePair> {
    let lines = parsed.lines_near(range.clone());
    let mut group_pairs = Vec::with_capacity(lines.iter().map(|l| l.inline_pairs.len()).sum());
    let mut seen = HashSet::with_capacity(group_pairs.capacity());
    for line in lines {
        expand_line_group_pairs(line, parsed, range.clone(), &mut group_pairs, &mut seen);
    }
    group_pairs
}

pub(super) fn add_contained_pairs<'a>(
    parsed: &'a ParsedDocument,
    replace: Range<usize>,
    group_pairs: &mut Vec<&'a InlinePair>,
) {
    let mut seen: HashSet<(usize, usize, InlineTagKind)> =
        group_pairs.iter().copied().map(pair_id).collect();
    for line in parsed.lines_near(replace.clone()) {
        for p in &line.inline_pairs {
            if p.kind == InlineTagKind::Link {
                continue;
            }
            if p.open_delim.start >= replace.start
                && span_close_end(parsed, p) <= replace.end
                && seen.insert(pair_id(p))
            {
                group_pairs.push(p);
            }
        }
    }
}

/// `line_col` answered from the parsed line table instead of rescanning the
/// text from the start.
pub(super) fn parsed_line_col(content: &str, parsed: &ParsedDocument, offset: usize) -> (usize, usize) {
    let offset = crate::model::text_ops::floor_char_boundary(content, offset.min(content.len()));
    let idx = parsed.lines.partition_point(|l| l.source.start <= offset);
    let Some(line) = idx.checked_sub(1).map(|i| &parsed.lines[i]) else {
        return line_col(content, offset);
    };
    let end = offset.min(line.source.end);
    (idx, content[line.source.start..end].chars().count() + 1)
}

/// The next source position at or after `pos` that is not inside a hidden
/// (WYSIWYG-invisible) range.
pub(super) fn next_visible_pos(parsed: &ParsedDocument, mut pos: usize) -> usize {
    while let Some(r) = hidden_range_at(parsed, pos, |r| pos >= r.start && pos < r.end) {
        pos = r.end;
    }
    pos
}

/// First hidden range of the line containing `pos` that satisfies `pred`.
/// Hidden ranges never cover the newline, so the line is enough.
pub(super) fn hidden_range_at(
    parsed: &ParsedDocument,
    pos: usize,
    pred: impl Fn(&std::ops::Range<usize>) -> bool,
) -> Option<std::ops::Range<usize>> {
    parsed
        .line_at_offset(pos)?
        .hidden_ranges
        .iter()
        .find(|r| pred(r))
        .cloned()
}

pub(super) fn outer_is_delimited(content: &str, start: usize, end: usize, prefix: &str, suffix: &str) -> bool {
    if start > content.len() || end > content.len() || start > end {
        return false;
    }
    if !content.is_char_boundary(start) || !content.is_char_boundary(end) {
        return false;
    }
    if !content[..start].ends_with(prefix) || !content[end..].starts_with(suffix) {
        return false;
    }
    if prefix == "*" {
        let extra_before = start > 1 && content.as_bytes().get(start - 2) == Some(&b'*');
        let extra_after = content.as_bytes().get(end + 1) == Some(&b'*');
        let triple_before = start > 2 && content.as_bytes().get(start - 3) == Some(&b'*');
        let triple_after = content.as_bytes().get(end + 2) == Some(&b'*');
        let triple = extra_before && triple_before && extra_after && triple_after;
        let single = !extra_before && !extra_after;
        triple || single
    } else if prefix == "`" {
        let extra_before = start > 1 && content.as_bytes().get(start - 2) == Some(&b'`');
        let extra_after = content.as_bytes().get(end + 1) == Some(&b'`');
        !extra_before && !extra_after
    } else {
        true
    }
}

pub(super) fn is_single_link(text: &str) -> Option<&str> {
    if !text.starts_with('[') || !text.ends_with(')') || text.starts_with("[^") {
        return None;
    }
    let close_bracket = text.find("](")?;
    let inner = &text[1..close_bracket];
    if inner.contains('[') || inner.contains(']') {
        return None;
    }
    let url_part = &text[close_bracket + 2..text.len() - 1];
    if url_part.contains(')') {
        return None;
    }
    Some(inner)
}

pub(super) fn unwrap_outer_link(content: &str, start: usize, end: usize) -> Option<(Range<usize>, String)> {
    if start == 0 || end > content.len() || start > end {
        return None;
    }
    if !content.is_char_boundary(start) || !content.is_char_boundary(end) {
        return None;
    }
    if !content[..start].ends_with('[') || !content[end..].starts_with("](") {
        return None;
    }
    let rest = &content[end + 2..];
    let close_idx = rest.find(')')?;
    let url = &rest[..close_idx];
    if url.contains('\n') || url.contains('(') || url.contains(']') {
        return None;
    }
    let edit_start = start - 1;
    let edit_end = end + 2 + close_idx + 1;
    Some((edit_start..edit_end, content[start..end].to_string()))
}
