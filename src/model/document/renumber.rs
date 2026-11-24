//! Keeping numbered lists in sequence after an edit.
//!
//! An edit that adds, removes or moves numbered items leaves the items after
//! it with stale numbers. Each such edit stages a set of *patches* (digit
//! runs to rewrite, all after the edited range) before it runs. `edit_inner`
//! folds them into the same edit, so the renumbering is part of one undo
//! step and the caret, which sits before every patch, never moves.
//!
//! Only a list that was already in sequence (consecutive numbers from its
//! first item) is renumbered, so lazy `1. 1. 1.` lists and irregular
//! numbering stay as typed. A list is the items at one indent and one
//! delimiter, with deeper lines (sub-lists, continuation text, blank lines)
//! skipped; a heading, other text at the list's own indent or less, a bullet,
//! a different delimiter or an item with less indent ends it.

use super::*;

/// A digit run to rewrite, as `(range, new digits)`.
pub(super) type Patch = (Range<usize>, String);

/// One numbered item of a run.
#[derive(Debug, Clone)]
pub(super) struct Member {
    pub line_start: usize,
    pub digits: Range<usize>,
    pub number: usize,
}

/// What a line means to the run being scanned.
enum Step {
    Member(Member),
    Skip,
    Stop,
}

/// True when the numbers count up by one from the first.
pub(super) fn in_sequence(members: &[Member]) -> bool {
    members
        .iter()
        .enumerate()
        .all(|(i, m)| m.number == members[0].number + i)
}

/// Patches that number `members` from `start`, skipping items already right.
/// Nothing at all when a number would outgrow the largest list marker.
pub(super) fn renumber_patches(members: &[Member], start: usize) -> Vec<Patch> {
    if start.saturating_add(members.len()) > MAX_LIST_NUMBER {
        return Vec::new();
    }
    members
        .iter()
        .enumerate()
        .filter(|(i, m)| m.number != start + i)
        .map(|(i, m)| (m.digits.clone(), (start + i).to_string()))
        .collect()
}

impl DocumentTab {
    fn classify_run_line(
        &self,
        parsed: &ParsedDocument,
        line: &ParsedLine,
        level: usize,
        punct: u8,
    ) -> Step {
        match line.kind {
            LineKind::Empty => Step::Skip,
            LineKind::Heading { .. } => Step::Stop,
            LineKind::ListItem { .. } => {
                let Some(found) = accepted_list_marker_in(parsed, &self.content, line.source.start)
                else {
                    return Step::Stop;
                };
                if found.indent_bytes > level {
                    return Step::Skip;
                }
                if found.indent_bytes < level || !found.ordered || found.punct != punct {
                    return Step::Stop;
                }
                let digits_at = line.source.start + found.indent_bytes;
                let digits = self.content[digits_at..]
                    .bytes()
                    .take_while(u8::is_ascii_digit)
                    .count();
                if digits == 0 {
                    return Step::Stop;
                }
                Step::Member(Member {
                    line_start: line.source.start,
                    digits: digits_at..digits_at + digits,
                    number: found.number,
                })
            }
            _ => {
                let text = &self.content[line.source.clone()];
                let lead = text.len() - text.trim_start_matches([' ', '\t']).len();
                if lead > level {
                    Step::Skip
                } else {
                    Step::Stop
                }
            }
        }
    }

    /// The items of the list at `level` and `punct` found walking from line
    /// `from` forward, or from line `from` back (`from` is None past the top).
    fn scan_run(
        &mut self,
        from: Option<usize>,
        forward: bool,
        level: usize,
        punct: u8,
    ) -> Vec<Member> {
        let Some(from) = from else {
            return Vec::new();
        };
        let parsed = self.parsed();
        let mut out = Vec::new();
        let mut visit =
            |line: &ParsedLine| match self.classify_run_line(&parsed, line, level, punct) {
                Step::Member(m) => {
                    out.push(m);
                    true
                }
                Step::Skip => true,
                Step::Stop => false,
            };
        if forward {
            for line in parsed.lines.iter().skip(from) {
                if !visit(line) {
                    break;
                }
            }
        } else {
            for line in parsed.lines[..(from + 1).min(parsed.lines.len())]
                .iter()
                .rev()
            {
                if !visit(line) {
                    break;
                }
            }
            out.reverse();
        }
        out
    }

    /// Index of the line starting at `offset`.
    fn line_index_at(&mut self, offset: usize) -> Option<usize> {
        let parsed = self.parsed();
        let line = parsed.line_at_offset(offset)?;
        Some(line.line_index as usize)
    }

    /// The whole list the numbered item at `line_start` belongs to, with the
    /// position of that item in it.
    pub(super) fn ordered_run(&mut self, line_start: usize) -> Option<(Vec<Member>, usize)> {
        if !self.file_type.supports_markdown_features() {
            return None;
        }
        let parsed = self.parsed();
        let marker = accepted_list_marker_in(&parsed, &self.content, line_start)?;
        if !marker.ordered {
            return None;
        }
        let index = parsed.line_at_offset(line_start)?.line_index as usize;
        let (level, punct) = (marker.indent_bytes, marker.punct);
        let mut run = self.scan_run(index.checked_sub(1), false, level, punct);
        let at = run.len();
        run.extend(self.scan_run(Some(index), true, level, punct));
        (run.get(at)?.line_start == line_start).then_some((run, at))
    }

    /// Stage patches so the next `edit` carries them. They are dropped when
    /// the text changes first.
    pub(super) fn stage_tail_patches(&mut self, patches: Vec<Patch>) {
        self.next_tail_patches = (!patches.is_empty()).then_some((self.revision, patches));
    }

    /// Enter in the middle of a numbered list opens a new item; the items
    /// after the current one move up by one.
    pub(super) fn stage_renumber_after_enter(
        &mut self,
        line_start: usize,
        tail_from: usize,
        number: usize,
    ) {
        let Some((run, _)) = self.ordered_run(line_start) else {
            return;
        };
        if !in_sequence(&run) {
            return;
        }
        let tail: Vec<Member> = run
            .into_iter()
            .filter(|m| m.line_start >= tail_from)
            .collect();
        self.stage_tail_patches(renumber_patches(&tail, number.saturating_add(2)));
    }

    /// An edit that takes numbered items' markers away (`range` covers them
    /// whole): the items after it shift down into the gap.
    pub(super) fn stage_renumber_for_removal(&mut self, range: &Range<usize>) {
        if range.is_empty() || !self.file_type.supports_markdown_features() {
            return;
        }
        let first = {
            let parsed = self.parsed();
            let Some(index) = parsed.line_at_offset(range.start).map(|l| l.line_index) else {
                return;
            };
            let mut found = None;
            for line in parsed.lines.iter().skip(index as usize) {
                if line.source.start >= range.end {
                    break;
                }
                if !matches!(line.kind, LineKind::ListItem { .. }) {
                    continue;
                }
                let Some(marker) =
                    accepted_list_marker_in(&parsed, &self.content, line.source.start)
                else {
                    continue;
                };
                if marker.ordered
                    && line.source.start + marker.indent_bytes >= range.start
                    && line.source.start + marker.marker_end <= range.end
                {
                    found = Some(line.source.start);
                    break;
                }
            }
            found
        };
        let Some(first) = first else { return };
        let Some((run, at)) = self.ordered_run(first) else {
            return;
        };
        if !in_sequence(&run) {
            return;
        }
        let start = run[at].number;
        let tail: Vec<Member> = run
            .into_iter()
            .filter(|m| m.line_start >= range.end)
            .collect();
        self.stage_tail_patches(renumber_patches(&tail, start));
    }

    /// Delete `range`, shifting down the numbered items after any item
    /// whose marker it takes away.
    pub fn edit_removing(&mut self, range: Range<usize>) {
        self.stage_renumber_for_removal(&range);
        self.edit(range, "");
    }

    /// Patches for the list that numbered items at `first_line_start` leave:
    /// the items after `block_end` shift down to the first leaver's number.
    pub(super) fn left_list_patches(
        &mut self,
        first_line_start: usize,
        block_end: usize,
    ) -> Vec<Patch> {
        let Some((run, at)) = self.ordered_run(first_line_start) else {
            return Vec::new();
        };
        if !in_sequence(&run) {
            return Vec::new();
        }
        let start = run[at].number;
        let tail: Vec<Member> = run
            .into_iter()
            .filter(|m| m.line_start >= block_end)
            .collect();
        renumber_patches(&tail, start)
    }

    /// Patches after Tab or Shift+Tab moved the items in `block` from
    /// `old_level` to `new_level`: the list they left closes the gap, and the
    /// list they joined counts on from `next`.
    pub(super) fn moved_items_patches(
        &mut self,
        block: Range<usize>,
        new_level: usize,
        punct: u8,
        next: usize,
    ) -> Vec<Patch> {
        let mut patches = self.left_list_patches(block.start, block.end);
        if self.head_numbers(block.start, new_level, punct).is_some() {
            patches.extend(self.joined_list_patches(block.end, new_level, punct, next));
        }
        patches.sort_by_key(|(digits, _)| digits.start);
        patches
    }

    /// The numbers of the list at `level` right above the line at
    /// `block_start`, when it is in sequence (empty when it has no items).
    pub(super) fn head_numbers(
        &mut self,
        block_start: usize,
        level: usize,
        punct: u8,
    ) -> Option<Vec<Member>> {
        let index = self.line_index_at(block_start)?;
        let head = self.scan_run(index.checked_sub(1), false, level, punct);
        in_sequence(&head).then_some(head)
    }

    /// Patches for the list that items moved to `level` join: the items
    /// after `block_end` at that level count on from `next`. Only when the
    /// items there were in sequence.
    pub(super) fn joined_list_patches(
        &mut self,
        block_end: usize,
        level: usize,
        punct: u8,
        next: usize,
    ) -> Vec<Patch> {
        let Some(index) = self.line_index_at(block_end) else {
            return Vec::new();
        };
        let tail = self.scan_run(Some(index + 1), true, level, punct);
        if !in_sequence(&tail) {
            return Vec::new();
        }
        renumber_patches(&tail, next)
    }

    /// Fold staged patches into an edit of `range`: the combined range and
    /// replacement, or None when nothing applies.
    pub(super) fn fold_tail_patches(
        &mut self,
        staged: Option<(u64, Vec<Patch>)>,
        range: &Range<usize>,
        new_text: &str,
    ) -> Option<(Range<usize>, String)> {
        let (revision, patches) = staged?;
        if revision != self.revision || patches.is_empty() {
            return None;
        }
        let mut end = range.end;
        let mut text = new_text.to_string();
        for (digits, replacement) in &patches {
            if digits.start < end
                || digits.end > self.content.len()
                || !self.content.is_char_boundary(digits.start)
                || !self.content.is_char_boundary(digits.end)
            {
                return None;
            }
            text.push_str(&self.content[end..digits.start]);
            text.push_str(replacement);
            end = digits.end;
        }
        Some((range.start..end, text))
    }
}
