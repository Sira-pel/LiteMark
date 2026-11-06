//! Styled-slice machinery that rewrites inline markup for format toggles.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FormatFlags {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub code: bool,
}

impl FormatFlags {
    pub(super) fn get(self, kind: InlineTagKind) -> bool {
        match kind {
            InlineTagKind::Bold => self.bold,
            InlineTagKind::Italic => self.italic,
            InlineTagKind::Underline => self.underline,
            InlineTagKind::Strike => self.strike,
            InlineTagKind::Code => self.code,
            InlineTagKind::Link => false,
        }
    }

    pub(super) fn any_set(self) -> bool {
        self.bold || self.italic || self.underline || self.strike || self.code
    }

    /// Bold, italic, and strike markers cannot open or close next to a space.
    pub(super) fn flanking_sensitive(self) -> bool {
        self.bold || self.italic || self.strike
    }

    /// Flags that may stay on a space peeled out of a flanking-sensitive span.
    /// Underline tags do not depend on spaces, so the space can remain inside
    /// `<u>` while sitting outside `*` / `**` / `~~`.
    pub(super) fn glue(self) -> Self {
        Self {
            underline: self.underline,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum SharedKind {
    Underline,
    Strike,
    BoldItalic,
    Bold,
    Italic,
    Code,
}

pub(super) fn flag_of(s: &WorkSlice, kind: SharedKind) -> bool {
    match kind {
        SharedKind::Underline => s.flags.underline,
        SharedKind::Strike => s.flags.strike,
        SharedKind::BoldItalic => s.flags.bold && s.flags.italic,
        SharedKind::Bold => s.flags.bold,
        SharedKind::Italic => s.flags.italic,
        SharedKind::Code => s.flags.code,
    }
}

pub(super) fn wrap_for_shared(kind: SharedKind) -> (String, String) {
    match kind {
        SharedKind::Underline => ("<u>".into(), "</u>".into()),
        SharedKind::Strike => ("~~".into(), "~~".into()),
        SharedKind::BoldItalic => ("***".into(), "***".into()),
        SharedKind::Bold => ("**".into(), "**".into()),
        SharedKind::Italic => ("*".into(), "*".into()),
        SharedKind::Code => ("`".into(), "`".into()),
    }
}

pub(super) fn clear_shared(mut flags: FormatFlags, kind: SharedKind) -> FormatFlags {
    match kind {
        SharedKind::Underline => flags.underline = false,
        SharedKind::Strike => flags.strike = false,
        SharedKind::BoldItalic => {
            flags.bold = false;
            flags.italic = false;
        }
        SharedKind::Bold => flags.bold = false,
        SharedKind::Italic => flags.italic = false,
        SharedKind::Code => flags.code = false,
    }
    flags
}

/// Longest consecutive run that shares a wrap-able flag, but is not the whole
/// list (`all_*` already handled that). Prefers outer kinds on a tie.
pub(super) fn longest_shared_flag_run(slices: &[WorkSlice]) -> Option<(SharedKind, usize, usize)> {
    let kinds = [
        SharedKind::Underline,
        SharedKind::Strike,
        SharedKind::BoldItalic,
        SharedKind::Bold,
        SharedKind::Italic,
        SharedKind::Code,
    ];
    let mut best: Option<(SharedKind, usize, usize, usize)> = None;
    for kind in kinds {
        let mut i = 0;
        while i < slices.len() {
            if !flag_of(&slices[i], kind) {
                i += 1;
                continue;
            }
            let mut j = i;
            while j + 1 < slices.len() && flag_of(&slices[j + 1], kind) {
                j += 1;
            }
            let len = j - i + 1;
            if len >= 2 && len < slices.len() {
                let better = best.is_none_or(|(_, _, _, best_len)| len > best_len);
                if better {
                    best = Some((kind, i, j, len));
                }
            }
            i = j + 1;
        }
    }
    best.map(|(k, i, j, _)| (k, i, j))
}

pub(super) fn segment_flags(seg: &StyledSegment) -> FormatFlags {
    FormatFlags {
        bold: seg.bold,
        italic: seg.italic,
        underline: seg.underline,
        strike: seg.strike,
        code: seg.code,
    }
}

/// One piece of text going through `render_styled_slices`, with the selected
/// byte range (if any) tracked independently of the wrap so adjacent pieces
/// with the same flags can merge into one span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkSlice {
    text: String,
    flags: FormatFlags,
    sel: Option<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSlice {
    pub text: String,
    pub flags: FormatFlags,
    pub is_selected: bool,
}

pub(super) fn wrap_delimiters(flags: &FormatFlags) -> (String, String) {
    let mut open = String::new();
    let mut close = String::new();
    if flags.underline {
        open.push_str("<u>");
        close.push_str("</u>");
    }
    if flags.strike {
        open.push_str("~~");
        close.insert_str(0, "~~");
    }
    if flags.bold && flags.italic {
        open.push_str("***");
        close.insert_str(0, "***");
    } else if flags.bold {
        open.push_str("**");
        close.insert_str(0, "**");
    } else if flags.italic {
        open.push('*');
        close.insert(0, '*');
    }
    if flags.code {
        open.push('`');
        close.insert(0, '`');
    }
    (open, close)
}

pub(super) fn append_work_slice(dest: &mut WorkSlice, src: &WorkSlice) {
    let at = dest.text.len();
    dest.text.push_str(&src.text);
    if let Some(sel) = &src.sel {
        dest.sel = Some(match &dest.sel {
            Some(prev) => prev.start..(at + sel.end),
            None => (at + sel.start)..(at + sel.end),
        });
    }
}

pub(super) fn merge_work_slices(slices: &[WorkSlice]) -> Vec<WorkSlice> {
    let mut merged: Vec<WorkSlice> = Vec::with_capacity(slices.len());
    for s in slices {
        if s.text.is_empty() {
            continue;
        }
        if let Some(last) = merged.last_mut() {
            if last.flags == s.flags {
                append_work_slice(last, s);
                continue;
            }
        }
        merged.push(s.clone());
    }
    merged
}

pub(super) fn sel_overlap(sel: &Option<Range<usize>>, start: usize, end: usize) -> Option<Range<usize>> {
    let sel = sel.as_ref()?;
    let a = sel.start.max(start).min(end);
    let b = sel.end.max(start).min(end);
    if b > a {
        Some((a - start)..(b - start))
    } else {
        None
    }
}

/// Pull leading/trailing spaces out of flanking-sensitive spans so a split
/// at a word edge cannot write `aa** bb` or `***aa **bb`. Underline stays
/// on the peeled space so `<u>` can still wrap the whole phrase.
pub(super) fn peel_flanking_whitespace(slices: Vec<WorkSlice>) -> Vec<WorkSlice> {
    if slices.len() <= 1 {
        return slices;
    }
    let mut out = Vec::new();
    for s in slices {
        if !s.flags.flanking_sensitive() || s.text.chars().all(char::is_whitespace) {
            out.push(s);
            continue;
        }
        let lead = s.text.len() - s.text.trim_start().len();
        let trail = s.text.len() - s.text.trim_end().len();
        if lead == 0 && trail == 0 {
            out.push(s);
            continue;
        }
        let glue = s.flags.glue();
        let core_end = s.text.len() - trail;
        if lead > 0 {
            out.push(WorkSlice {
                text: s.text[..lead].to_string(),
                flags: glue,
                sel: sel_overlap(&s.sel, 0, lead),
            });
        }
        if core_end > lead {
            out.push(WorkSlice {
                text: s.text[lead..core_end].to_string(),
                flags: s.flags,
                sel: sel_overlap(&s.sel, lead, core_end),
            });
        }
        if trail > 0 {
            out.push(WorkSlice {
                text: s.text[core_end..].to_string(),
                flags: glue,
                sel: sel_overlap(&s.sel, core_end, s.text.len()),
            });
        }
    }
    merge_work_slices(&out)
}

pub(super) fn can_coalesce_ws_triple(left: &WorkSlice, mid: &WorkSlice, right: &WorkSlice) -> bool {
    let mid_ws = !mid.text.is_empty() && mid.text.chars().all(char::is_whitespace);
    let flags_match = left.flags == right.flags && left.flags.any_set();
    let mid_ok = mid.flags == FormatFlags::default() || mid.flags == left.flags;
    mid_ws && flags_match && mid_ok
}

/// `[bold "aa"] [plain " "] [bold "bb"]` becomes one span so turning a
/// format back on at a neighbor merges instead of writing `**aa**** bb**`.
pub(super) fn coalesce_ws_between_matching(slices: Vec<WorkSlice>) -> Vec<WorkSlice> {
    let mut out: Vec<WorkSlice> = Vec::with_capacity(slices.len());
    for s in slices {
        out.push(s);
        while out.len() >= 3 {
            let n = out.len();
            if !can_coalesce_ws_triple(&out[n - 3], &out[n - 2], &out[n - 1]) {
                break;
            }
            let right = out.pop().unwrap();
            let mid = out.pop().unwrap();
            let left = out.last_mut().unwrap();
            append_work_slice(left, &mid);
            append_work_slice(left, &right);
        }
    }
    out
}

pub(super) fn map_work_flags(
    slices: &[WorkSlice],
    mut f: impl FnMut(FormatFlags) -> FormatFlags,
) -> Vec<WorkSlice> {
    slices
        .iter()
        .map(|s| WorkSlice {
            text: s.text.clone(),
            flags: f(s.flags),
            sel: s.sel.clone(),
        })
        .collect()
}

pub(super) fn mark_work_sel(sel_range: &mut Option<Range<usize>>, at: usize, s: &WorkSlice) {
    if let Some(sel) = &s.sel {
        let piece = (at + sel.start)..(at + sel.end);
        *sel_range = Some(match sel_range.take() {
            Some(prev) => prev.start.min(piece.start)..prev.end.max(piece.end),
            None => piece,
        });
    }
}

pub(super) fn union_sel(sel_range: &mut Option<Range<usize>>, piece: Range<usize>) {
    *sel_range = Some(match sel_range.take() {
        Some(prev) => prev.start.min(piece.start)..prev.end.max(piece.end),
        None => piece,
    });
}

pub(super) fn mark_wrapped_sel(
    sel_range: &mut Option<Range<usize>>,
    offset: usize,
    prefix_len: usize,
    suffix_len: usize,
    s: &WorkSlice,
    include_wraps: bool,
) {
    if let Some(sel) = &s.sel {
        if include_wraps && sel.start == 0 && sel.end == s.text.len() {
            union_sel(
                sel_range,
                offset..(offset + prefix_len + s.text.len() + suffix_len),
            );
        } else {
            mark_work_sel(sel_range, offset + prefix_len, s);
        }
    }
}

pub(super) fn wrap_work_slice(
    s: &WorkSlice,
    prefix: &str,
    suffix: &str,
    offset: usize,
    sel_range: &mut Option<Range<usize>>,
    include_wraps: bool,
) -> String {
    mark_wrapped_sel(
        sel_range,
        offset,
        prefix.len(),
        suffix.len(),
        s,
        include_wraps,
    );
    format!("{}{}{}", prefix, s.text, suffix)
}

pub(super) fn render_styled_slices(
    slices: &[StyledSlice],
    sel_range: &mut Option<Range<usize>>,
    offset: usize,
) -> String {
    render_styled_slices_opts(slices, sel_range, offset, false, false)
}

pub(super) fn render_styled_slices_opts(
    slices: &[StyledSlice],
    sel_range: &mut Option<Range<usize>>,
    offset: usize,
    glued_left: bool,
    include_wraps_in_sel: bool,
) -> String {
    let work: Vec<WorkSlice> = slices
        .iter()
        .map(|s| WorkSlice {
            text: s.text.clone(),
            flags: s.flags,
            sel: if s.is_selected {
                Some(0..s.text.len())
            } else {
                None
            },
        })
        .collect();
    render_work_slices(&work, sel_range, offset, glued_left, include_wraps_in_sel)
}

pub(super) fn slice_has_edge_ws(s: &WorkSlice) -> bool {
    s.text.starts_with(char::is_whitespace) || s.text.ends_with(char::is_whitespace)
}

pub(super) fn render_work_slices(
    slices: &[WorkSlice],
    sel_range: &mut Option<Range<usize>>,
    offset: usize,
    glued_left: bool,
    include_wraps_in_sel: bool,
) -> String {
    // Merge matching flags first so turning a format back on joins the
    // neighbor. Do not peel yet: shared outer formats (`*…*`, `<u>…</u>`)
    // must still wrap spaces, which is valid as long as the opener/closer
    // sit against non-space at the ends.
    let merged = coalesce_ws_between_matching(merge_work_slices(slices));

    if merged.is_empty() {
        return String::new();
    }
    if merged.len() == 1 {
        let s = &merged[0];
        let (prefix, suffix) = wrap_delimiters(&s.flags);
        return wrap_work_slice(s, &prefix, &suffix, offset, sel_range, include_wraps_in_sel);
    }

    let all_underline = merged.iter().all(|s| s.flags.underline);
    let all_strike = merged.iter().all(|s| s.flags.strike);
    let all_code = merged.iter().all(|s| s.flags.code);
    let all_bold = merged.iter().all(|s| s.flags.bold);
    let all_italic = merged.iter().all(|s| s.flags.italic);

    if all_underline {
        let inner = map_work_flags(&merged, |mut f| {
            f.underline = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 3, false, include_wraps_in_sel);
        return format!("<u>{}</u>", inner_str);
    }

    if all_strike {
        let inner = map_work_flags(&merged, |mut f| {
            f.strike = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 2, false, include_wraps_in_sel);
        return format!("~~{}~~", inner_str);
    }

    if all_bold && all_italic {
        let inner = map_work_flags(&merged, |mut f| {
            f.bold = false;
            f.italic = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 3, false, include_wraps_in_sel);
        return format!("***{}***", inner_str);
    }

    if all_italic {
        let is_middle = merged.len() == 3
            && !merged.iter().any(|s| {
                s.flags.strike || s.flags.underline || s.flags.code || slice_has_edge_ws(s)
            })
            && merged[0].flags.bold
            && !merged[1].flags.bold
            && merged[2].flags.bold;
        if is_middle {
            if glued_left {
                let mut out = String::new();
                for s in &merged {
                    let (prefix, suffix) = if s.flags.bold {
                        ("***", "***")
                    } else {
                        ("*", "*")
                    };
                    mark_work_sel(sel_range, offset + out.len() + prefix.len(), s);
                    out.push_str(prefix);
                    out.push_str(&s.text);
                    out.push_str(suffix);
                }
                return out;
            }
            let mut out = String::from("_");
            for s in &merged {
                let prefix = if s.flags.bold { "**" } else { "" };
                let suffix = if s.flags.bold { "**" } else { "" };
                mark_work_sel(sel_range, offset + out.len() + prefix.len(), s);
                out.push_str(prefix);
                out.push_str(&s.text);
                out.push_str(suffix);
            }
            out.push('_');
            return out;
        }

        let inner = map_work_flags(&merged, |mut f| {
            f.italic = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 1, false, include_wraps_in_sel);
        return format!("*{}*", inner_str);
    }

    if all_bold {
        let is_middle = merged.len() == 3
            && !merged.iter().any(|s| {
                s.flags.strike || s.flags.underline || s.flags.code || slice_has_edge_ws(s)
            })
            && merged[0].flags.italic
            && !merged[1].flags.italic
            && merged[2].flags.italic;
        if is_middle {
            let mut out = String::new();
            for s in &merged {
                let (prefix, suffix) = if s.flags.italic {
                    ("_**", "**_")
                } else {
                    ("**", "**")
                };
                mark_work_sel(sel_range, offset + out.len() + prefix.len(), s);
                out.push_str(prefix);
                out.push_str(&s.text);
                out.push_str(suffix);
            }
            return out;
        }

        let inner = map_work_flags(&merged, |mut f| {
            f.bold = false;
            f
        });
        let inner_str = render_work_slices(&inner, sel_range, offset + 2, false, include_wraps_in_sel);
        return format!("**{}**", inner_str);
    }

    if all_code {
        // A code span's content is literal, so it must never contain other
        // delimiters. Keep one backtick span only when the non-code flags are
        // identical across the slices (they then differ only by selection);
        // otherwise close and reopen the code span around every non-code
        // change, keeping the backticks adjacent to the text.
        let non_code_equal = merged.iter().all(|s| {
            s.flags.bold == merged[0].flags.bold
                && s.flags.italic == merged[0].flags.italic
                && s.flags.underline == merged[0].flags.underline
                && s.flags.strike == merged[0].flags.strike
        });
        if non_code_equal {
            let inner = map_work_flags(&merged, |mut f| {
                f.code = false;
                f
            });
            let inner_str = render_work_slices(&inner, sel_range, offset + 1, false, include_wraps_in_sel);
            return format!("`{}`", inner_str);
        }
    }

    if let Some((kind, mut i, mut j)) = longest_shared_flag_run(&merged) {
        let ws = |s: &WorkSlice| !s.text.is_empty() && s.text.chars().all(char::is_whitespace);
        let orig_i = i;
        let orig_j = j;
        while j > i && ws(&merged[j]) {
            j -= 1;
        }
        while i < j && ws(&merged[i]) {
            i += 1;
        }
        if j > i && (i > 0 || j + 1 < merged.len()) {
            let (prefix, suffix) = wrap_for_shared(kind);
            // Collide against the slices that stay outside the original run,
            // not the whitespace just peeled off (that whitespace loses this
            // flag and will not wrap).
            let left_collide = orig_i > 0 && {
                let prev_close = wrap_delimiters(&merged[orig_i - 1].flags).1;
                !prev_close.is_empty()
                    && !prefix.is_empty()
                    && prev_close.chars().next_back() == prefix.chars().next()
            };
            let right_collide = orig_j + 1 < merged.len() && {
                let next_open = wrap_delimiters(&merged[orig_j + 1].flags).0;
                !suffix.is_empty()
                    && !next_open.is_empty()
                    && suffix.chars().next_back() == next_open.chars().next()
            };
            // Skip a wrap whose new markers would sit against a neighbor's
            // `*`/`**` and form `*a***`; peel + alternate delimiters instead.
            if !left_collide && !right_collide {
                // Whitespace peeled out of the wrap must lose that flag, or a
                // trailing space stays `** **now` instead of `** …** now`.
                let mut work = merged.clone();
                for s in &mut work[orig_i..i] {
                    s.flags = clear_shared(s.flags, kind);
                }
                if orig_j < work.len() {
                    for s in &mut work[j + 1..=orig_j] {
                        s.flags = clear_shared(s.flags, kind);
                    }
                }
                let left = render_work_slices(
                    &work[..i],
                    sel_range,
                    offset,
                    glued_left,
                    include_wraps_in_sel,
                );
                let inner = map_work_flags(&work[i..=j], |f| clear_shared(f, kind));
                let mid = render_work_slices(
                    &inner,
                    sel_range,
                    offset + left.len() + prefix.len(),
                    false,
                    include_wraps_in_sel,
                );
                let right = render_work_slices(
                    &work[j + 1..],
                    sel_range,
                    offset + left.len() + prefix.len() + mid.len() + suffix.len(),
                    false,
                    include_wraps_in_sel,
                );
                return format!("{left}{prefix}{mid}{suffix}{right}");
            }
        }
    }

    // Formats diverge here, so peel spaces out of flanking-sensitive spans
    // before writing markers. That turns `** aa` / `aa **` into ` **aa` /
    // `aa** ` and stops `***` / `****` from forming at a word edge.
    let merged = coalesce_ws_between_matching(peel_flanking_whitespace(merged));
    if merged.len() <= 1 {
        return render_work_slices(&merged, sel_range, offset, glued_left, include_wraps_in_sel);
    }

    // Adjacent spans whose delimiters collide (a closer and the next opener)
    // sharing a character) merge into one ambiguous run that CommonMark's
    // delimiter arithmetic mis-parses. Resolve collisions before rendering:
    // switch the later span to a delimiter style whose outermost character
    // differs (`__`/`_` for bold and italic, strike nested inside the other
    // formats), unless plain text follows it - then switch the earlier one,
    // whose new closer sits against the next span's opener instead.
    let mut use_alt = vec![false; merged.len()];
    for i in 1..merged.len() {
        let (_, prev_close) = wrap_delimiters(&merged[i - 1].flags);
        let (next_open, _) = wrap_delimiters(&merged[i].flags);
        let collide = !prev_close.is_empty()
            && !next_open.is_empty()
            && prev_close.ends_with(next_open.chars().next().unwrap());
        if !collide {
            continue;
        }
        let avoid = next_open.chars().next().unwrap();
        let alt_available = alternate_wrap(&merged[i].flags, avoid).is_some();
        let plain_follows =
            i + 1 == merged.len() || wrap_delimiters(&merged[i + 1].flags).0.is_empty();
        if alt_available && !plain_follows {
            use_alt[i] = true;
        } else if alt_available && alternate_wrap(&merged[i - 1].flags, avoid).is_some() {
            // Plain text follows this span: its underscore closer would go
            // intraword. Switch the earlier span instead; its new closer sits
            // against this span's opener, which is a delimiter character.
            use_alt[i - 1] = true;
        } else if alt_available {
            use_alt[i] = true;
        }
    }
    let mut out = String::new();
    for (idx, s) in merged.iter().enumerate() {
        let (prefix, suffix) = if use_alt[idx] {
            let avoid = if idx > 0 {
                wrap_delimiters(&merged[idx - 1].flags)
                    .1
                    .chars()
                    .next_back()
                    .unwrap_or('*')
            } else {
                '*'
            };
            alternate_wrap(&s.flags, avoid).unwrap_or_else(|| wrap_delimiters(&s.flags))
        } else {
            wrap_delimiters(&s.flags)
        };
        mark_wrapped_sel(
            sel_range,
            offset + out.len(),
            prefix.len(),
            suffix.len(),
            s,
            include_wraps_in_sel,
        );
        out.push_str(&prefix);
        out.push_str(&s.text);
        out.push_str(&suffix);
    }
    out
}

/// An alternate delimiter wrapping for `flags` whose outermost delimiter
/// character differs from `avoid`, used where two adjacent spans would
/// otherwise merge their closing and opening runs into one ambiguous run.
/// Code spans stay innermost in every variant; returns None when no
/// alternate exists.
pub(super) fn alternate_wrap(flags: &FormatFlags, avoid: char) -> Option<(String, String)> {
    let mut open = String::new();
    let mut close = String::new();
    if avoid == '*' && (flags.bold || flags.italic) {
        // Bold and italic have a second syntax each; the underscore style
        // moves the boundary character off `*`.
        if flags.bold {
            open.push_str("__");
            close.insert_str(0, "__");
        } else {
            open.push('_');
            close.insert(0, '_');
        }
        if flags.bold && flags.italic {
            open.push('*');
            close.insert(0, '*');
        }
        if flags.code {
            open.push('`');
            close.insert(0, '`');
        }
        return Some((open, close));
    }
    if avoid == '~' && flags.strike {
        // Strike has no second syntax; nest it inside the other formats.
        let mut outer = false;
        if flags.underline {
            open.push_str("<u>");
            close.insert_str(0, "</u>");
            outer = true;
        }
        if flags.bold && flags.italic {
            open.push_str("***");
            close.insert_str(0, "***");
            outer = true;
        } else if flags.bold {
            open.push_str("**");
            close.insert_str(0, "**");
            outer = true;
        } else if flags.italic {
            open.push('*');
            close.insert(0, '*');
            outer = true;
        }
        if outer {
            open.push_str("~~");
            close.insert_str(0, "~~");
            return Some((open, close));
        }
    }
    None
}

pub(super) fn render_styled_slices_with_selection_glued(
    slices: &[StyledSlice],
    glued_left: bool,
) -> (String, Range<usize>) {
    render_styled_slices_with_selection_opts(slices, glued_left, false)
}

pub(super) fn render_styled_slices_with_selection_opts(
    slices: &[StyledSlice],
    glued_left: bool,
    include_wraps_in_sel: bool,
) -> (String, Range<usize>) {
    let mut sel = None;
    let res = render_styled_slices_opts(slices, &mut sel, 0, glued_left, include_wraps_in_sel);
    let range = sel.unwrap_or(0..res.len());
    (res, range)
}

/// The raw slice for a styled segment, including a backslash that escapes
/// its first character: escapes are consumed by the parser, so rebuilding a
/// region from segments must re-emit them or the escape is lost.
pub(super) fn segment_raw_range(content: &str, source: &Range<usize>) -> Range<usize> {
    let mut start = source.start;
    if start > 0
        && content.as_bytes().get(start - 1) == Some(&b'\\')
        && content[start..].chars().next().is_some_and(|c| c.is_ascii_punctuation())
        && content.as_bytes().get(start - 2) != Some(&b'\\')
    {
        start -= 1;
    }
    start..source.end
}

/// One line's inline toggle: (replaced source range, replacement, selected
/// text inside the replacement, start of the selected text in the source).
pub(super) type InlineRewrite = (Range<usize>, String, Range<usize>, usize);

/// Whether each rewrite parses the way it was meant to: the line keeps its
/// block kind, and every visible segment of the selected text has the format
/// when `on`, or lacks it when not. All rewrites are applied together and
/// parsed once, so a line is judged in its real block context (a nested list,
/// a quote). Rewrites must not overlap, which holds because each sits on its
/// own line or table cell.
pub(super) fn verify_inline_rewrites(
    content: &str,
    parsed: &ParsedDocument,
    rewrites: &[Option<InlineRewrite>],
    kind: InlineTagKind,
    on: bool,
) -> Vec<bool> {
    let mut next = String::with_capacity(content.len() + rewrites.len() * 8);
    let mut cursor = 0;
    let mut placed: Vec<Option<(usize, Range<usize>)>> = Vec::with_capacity(rewrites.len());
    for rewrite in rewrites {
        let Some((range, text, sel, _)) = rewrite else {
            placed.push(None);
            continue;
        };
        next.push_str(&content[cursor..range.start]);
        let start = next.len();
        next.push_str(text);
        cursor = range.end;
        placed.push(Some((range.start, start + sel.start..start + sel.end)));
    }
    next.push_str(&content[cursor..]);
    let reparsed = crate::markdown::parse_document(&next);

    placed
        .into_iter()
        .map(|place| {
            let Some((old_start, sel)) = place else {
                return false;
            };
            let (Some(old_line), Some(new_line)) =
                (parsed.line_at_offset(old_start), reparsed.line_at_offset(sel.start))
            else {
                return false;
            };
            if std::mem::discriminant(&old_line.kind) != std::mem::discriminant(&new_line.kind) {
                return false;
            }
            let mut any = false;
            let all_match = new_line
                .styled_segments
                .iter()
                .filter(|s| {
                    s.source.start < sel.end
                        && s.source.end > sel.start
                        && !next[s.source.clone()].trim().is_empty()
                })
                .all(|s| {
                    any = true;
                    segment_flags(s).get(kind) == on
                });
            // Turning off only needs no segment left formatted: a line may show
            // nothing once its markers go (`~~~`, a `[^1]: note` definition).
            all_match && (any || !on)
        })
        .collect()
}

pub(super) fn apply_toggle(flags: &mut FormatFlags, kind: InlineTagKind, on: bool) {
    match kind {
        InlineTagKind::Bold => flags.bold = on,
        InlineTagKind::Italic => flags.italic = on,
        InlineTagKind::Underline => flags.underline = on,
        InlineTagKind::Strike => flags.strike = on,
        InlineTagKind::Code => flags.code = on,
        InlineTagKind::Link => {}
    }
}

pub(super) fn segment_flags_at(segs: &[&StyledSegment], pos: usize) -> Option<FormatFlags> {
    let i = segs.partition_point(|s| s.source.start <= pos);
    if i == 0 {
        return None;
    }
    let s = segs[i - 1];
    if pos < s.source.end {
        Some(segment_flags(s))
    } else {
        None
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct FlagCounts {
    bold: u16,
    italic: u16,
    underline: u16,
    strike: u16,
    code: u16,
}

impl FlagCounts {
    pub(super) fn apply(&mut self, kind: InlineTagKind, open: bool) {
        let slot = match kind {
            InlineTagKind::Bold => &mut self.bold,
            InlineTagKind::Italic => &mut self.italic,
            InlineTagKind::Underline => &mut self.underline,
            InlineTagKind::Strike => &mut self.strike,
            InlineTagKind::Code => &mut self.code,
            InlineTagKind::Link => return,
        };
        if open {
            *slot = slot.saturating_add(1);
        } else {
            *slot = slot.saturating_sub(1);
        }
    }

    pub(super) fn flags(self) -> FormatFlags {
        FormatFlags {
            bold: self.bold > 0,
            italic: self.italic > 0,
            underline: self.underline > 0,
            strike: self.strike > 0,
            code: self.code > 0,
        }
    }
}

/// Open/close events sorted by position so flags at successive offsets are O(1)
/// amortized. Closes (`-1`) sort before opens (`1`) at the same byte because
/// `pos < close.end` and `pos >= open.start`.
pub(super) struct FlagSweep {
    events: Vec<(usize, i8, InlineTagKind)>,
    i: usize,
    counts: FlagCounts,
}

impl FlagSweep {
    pub(super) fn new(pairs: &[&InlinePair]) -> Self {
        let mut events = Vec::with_capacity(pairs.len() * 2);
        for p in pairs {
            if p.kind == InlineTagKind::Link {
                continue;
            }
            events.push((p.open_delim.start, 1, p.kind));
            events.push((p.close_delim.end, -1, p.kind));
        }
        events.sort_unstable_by_key(|e| (e.0, e.1));
        Self {
            events,
            i: 0,
            counts: FlagCounts::default(),
        }
    }

    pub(super) fn flags_at(&mut self, pos: usize) -> FormatFlags {
        while self.i < self.events.len() && self.events[self.i].0 <= pos {
            let (_, delta, kind) = self.events[self.i];
            self.counts.apply(kind, delta > 0);
            self.i += 1;
        }
        self.counts.flags()
    }
}

/// Openers and closers of every non-link span the caret sits in, merged into
/// adjacent runs so nested `**… *…* …**` closes as `***` / `***`.
pub(super) fn spanning_marker_runs(
    content: &str,
    parsed: &ParsedDocument,
    cursor: usize,
) -> Option<(String, String)> {
    let mut spanning: Vec<InlinePair> = line_pair_refs(parsed, cursor)
        .iter()
        .filter(|p| {
            p.kind != InlineTagKind::Link
                && cursor > p.open_delim.start
                && cursor < p.close_delim.end
                && cursor >= p.inner.start
                && cursor <= p.inner.end
        })
        .cloned()
        .collect();
    if spanning.is_empty() {
        return None;
    }
    spanning.sort_by_key(|p| p.open_delim.start);
    let open_ranges =
        merge_adjacent_ranges(spanning.iter().map(|p| p.open_delim.clone()).collect());
    let close_ranges =
        merge_adjacent_ranges(spanning.iter().map(|p| p.close_delim.clone()).collect());
    let openers: String = open_ranges.iter().map(|r| &content[r.clone()]).collect();
    let closers: String = close_ranges.iter().map(|r| &content[r.clone()]).collect();
    Some((openers, closers))
}

pub(super) fn merge_adjacent_ranges(mut rs: Vec<Range<usize>>) -> Vec<Range<usize>> {
    if rs.is_empty() {
        return rs;
    }
    rs.sort_by_key(|r| r.start);
    let mut out = vec![rs[0].clone()];
    for r in rs.into_iter().skip(1) {
        let last = out.last_mut().unwrap();
        if r.start <= last.end {
            last.end = last.end.max(r.end);
        } else {
            out.push(r);
        }
    }
    out
}

pub(super) fn merge_skip_ranges(mut rs: Vec<Range<usize>>) -> Vec<Range<usize>> {
    rs.retain(|r| r.end > r.start);
    if rs.len() <= 1 {
        return rs;
    }
    rs.sort_by_key(|r| (r.start, r.end));
    let mut out: Vec<Range<usize>> = Vec::with_capacity(rs.len());
    for r in rs {
        if let Some(last) = out.last_mut() {
            if r.start <= last.end {
                last.end = last.end.max(r.end);
                continue;
            }
        }
        out.push(r);
    }
    out
}

pub(super) fn skip_end_at(skips: &[Range<usize>], pos: usize) -> Option<usize> {
    let i = skips.partition_point(|r| r.start <= pos);
    if i == 0 {
        return None;
    }
    let r = &skips[i - 1];
    if pos < r.end {
        Some(r.end)
    } else {
        None
    }
}

pub(super) fn selected_link_markup_ranges(pairs: &[&InlinePair], p_range: Range<usize>) -> Vec<Range<usize>> {
    let mut rs = Vec::new();
    for p in pairs {
        if p.kind != InlineTagKind::Link
            || p.inner.start < p_range.start
            || p.inner.end > p_range.end
            || p.inner.end <= p.inner.start
        {
            continue;
        }
        if p.open_delim.start < p.inner.start {
            rs.push(p.open_delim.start..p.inner.start);
        }
        if p.inner.end < p.close_delim.end {
            rs.push(p.inner.end..p.close_delim.end);
        }
    }
    merge_skip_ranges(rs)
}

/// Rebuild a source range from styled segments **and** the hidden markdown
/// between them (link/image syntax, HTML, escapes), skipping only the
/// delimiter runs in `skip` (the emphasis markers being rewritten).
pub(super) fn collect_rewrite_slices(
    content: &str,
    pairs: &[&InlinePair],
    segs: &[&StyledSegment],
    replace: Range<usize>,
    p_range: Range<usize>,
    skip: &[Range<usize>],
    toggle: Option<(InlineTagKind, bool)>,
) -> Vec<StyledSlice> {
    let skip = merge_skip_ranges(skip.to_vec());
    let mut segs = segs.to_vec();
    segs.sort_by_key(|s| (s.source.start, s.source.end));
    let mut sweep = FlagSweep::new(pairs);
    let link_markup = selected_link_markup_ranges(pairs, p_range.clone());
    let mut bounds = Vec::with_capacity(
        8 + skip.len() * 2 + segs.len() * 2 + replace.end.saturating_sub(replace.start) / 4,
    );
    bounds.push(replace.start);
    bounds.push(replace.end);
    bounds.push(p_range.start.clamp(replace.start, replace.end));
    bounds.push(p_range.end.clamp(replace.start, replace.end));
    for r in &skip {
        if r.start > replace.start && r.start < replace.end {
            bounds.push(r.start);
        }
        if r.end > replace.start && r.end < replace.end {
            bounds.push(r.end);
        }
    }
    for seg in &segs {
        let raw = segment_raw_range(content, &seg.source);
        if raw.start > replace.start && raw.start < replace.end {
            bounds.push(raw.start);
        }
        if raw.end > replace.start && raw.end < replace.end {
            bounds.push(raw.end);
        }
    }
    // Split whitespace from words so a selected `" wo"` does not keep the
    // leading space inside `* wo*` (nf2). Interior spaces between matching
    // flags are coalesced again when wrapping.
    let mut pos = replace.start;
    while pos < replace.end {
        if let Some(end) = skip_end_at(&skip, pos) {
            pos = end.max(pos + 1);
            continue;
        }
        let ch = match content[pos..].chars().next() {
            Some(c) => c,
            None => break,
        };
        let next = pos + ch.len_utf8();
        if next < replace.end {
            let nch = match content[next..].chars().next() {
                Some(c) => c,
                None => break,
            };
            if ch == '\n' || nch == '\n' || ch.is_whitespace() != nch.is_whitespace() {
                bounds.push(next);
            }
        }
        pos = next;
    }
    bounds.sort_unstable();
    bounds.dedup();

    let turning_off = toggle.is_some_and(|(_, on)| !on);
    let mut slices: Vec<StyledSlice> = Vec::with_capacity(bounds.len().saturating_sub(1));
    for w in bounds.windows(2) {
        let a = w[0];
        let b = w[1];
        if a >= b || skip_end_at(&skip, a).is_some() {
            continue;
        }
        let piece = &content[a..b];
        let is_break = piece == "\n";
        let is_ws = !is_break && piece.chars().all(char::is_whitespace);
        let is_sel = a < p_range.end && b > p_range.start;
        let mut flags = if is_break {
            FormatFlags::default()
        } else {
            let swept = sweep.flags_at(a);
            segment_flags_at(&segs, a).unwrap_or(swept)
        };
        let in_selected_link_markup = turning_off && skip_end_at(&link_markup, a).is_some();
        // Turning a format on skips whitespace so flanking spaces stay
        // outside `*`/`**`/`~~`. Turning one off must still clear it from
        // selected spaces, or `**RAM** **usage**` becomes `RAM** **usage`.
        if (is_sel && !is_break && (!is_ws || turning_off)) || in_selected_link_markup {
            if let Some((kind, on)) = toggle {
                apply_toggle(&mut flags, kind, on);
            }
        }
        if let Some(last) = slices.last_mut() {
            if last.flags == flags && last.is_selected == is_sel && !is_break && last.text != "\n" {
                last.text.push_str(piece);
                continue;
            }
        }
        slices.push(StyledSlice {
            text: piece.to_string(),
            flags,
            is_selected: is_sel,
        });
    }
    slices
}

pub(super) fn extend_or_splice_span(
    content: &str,
    parsed: &ParsedDocument,
    pending: &ActiveFormats,
    caret: usize,
    text: &str,
) -> Option<(Range<usize>, String, usize)> {
    if caret == 0 {
        return None;
    }
    let has_any_armed =
        pending.bold || pending.italic || pending.underline || pending.strike || pending.code;
    if !has_any_armed {
        return None;
    }
    let has_space = content.as_bytes().get(caret - 1) == Some(&b' ');
    let closer_end = if has_space { caret - 1 } else { caret };
    // Two continuation shapes: typing right after a span's closing run (or
    // after the trailing space it owns), and typing at the end of a span's
    // content while formats are armed - the second keeps the span's own
    // formats on the new text instead of nesting raw markers inside it.
    // Several stacked pairs can share a closing run; prefer one whose group
    // carries every armed format.
    let mut after_candidates: Vec<InlinePair> = line_pair_refs(parsed, closer_end)
        .iter()
        .rev()
        .filter(|p| {
            p.kind != InlineTagKind::Link
                && (span_close_end(parsed, p) == closer_end
                    || close_run_reaches(content, p, closer_end))
        })
        .cloned()
        .collect();
    let is_armed = |k: InlineTagKind| match k {
        InlineTagKind::Bold => pending.bold,
        InlineTagKind::Italic => pending.italic,
        InlineTagKind::Underline => pending.underline,
        InlineTagKind::Strike => pending.strike,
        InlineTagKind::Code => pending.code,
        InlineTagKind::Link => false,
    };
    // Several stacked pairs can share a closing run; prefer one whose group
    // carries every armed format. A pair whose closer sits inside that run
    // (it ends at the caret too) is also a candidate.
    after_candidates.sort_by_key(|p| {
        let carries_armed = [
            InlineTagKind::Bold,
            InlineTagKind::Italic,
            InlineTagKind::Underline,
            InlineTagKind::Strike,
            InlineTagKind::Code,
        ]
        .iter()
        .all(|&k| {
            !is_armed(k)
                || line_pair_refs(parsed, closer_end)
                    .iter()
                    .any(|q| q.kind == k && q.inner == p.inner)
        });
        if carries_armed {
            0
        } else {
            1
        }
    });
    for a in &after_candidates {
        if let Some(r) = splice_for_anchor(content, parsed, pending, caret, text, a, false) {
            return Some(r);
        }
    }
    let inside_candidate = line_pair_refs(parsed, caret)
        .iter()
        .rev()
        .find(|p| {
            p.kind != InlineTagKind::Link && p.inner.end == caret && p.inner.end > p.inner.start
        })
        .cloned()?;
    splice_for_anchor(content, parsed, pending, caret, text, &inside_candidate, true)
}

/// True when the maximal delimiter run that contains `pair`'s closer extends
/// up to `caret`, i.e. the caret stands right after the shared closing run.
pub(super) fn close_run_reaches(content: &str, pair: &InlinePair, caret: usize) -> bool {
    let bytes = content.as_bytes();
    let Some(delim) = bytes.get(pair.close_delim.start).copied() else {
        return false;
    };
    pair.close_delim.end < caret
        && bytes[pair.close_delim.end..caret].iter().all(|&b| b == delim)
}

pub(super) fn splice_for_anchor(
    content: &str,
    parsed: &ParsedDocument,
    pending: &ActiveFormats,
    caret: usize,
    text: &str,
    anchor: &InlinePair,
    inside: bool,
) -> Option<(Range<usize>, String, usize)> {
    let group: Vec<InlinePair> = line_pair_refs(parsed, anchor.inner.start)
        .iter()
        .filter(|p| p.kind != InlineTagKind::Link && p.inner == anchor.inner)
        .cloned()
        .collect();
    if group.is_empty() {
        return None;
    }
    let is_armed = |k: InlineTagKind| match k {
        InlineTagKind::Bold => pending.bold,
        InlineTagKind::Italic => pending.italic,
        InlineTagKind::Underline => pending.underline,
        InlineTagKind::Strike => pending.strike,
        InlineTagKind::Code => pending.code,
        InlineTagKind::Link => false,
    };
    let all_armed_in_group = [
        InlineTagKind::Bold,
        InlineTagKind::Italic,
        InlineTagKind::Underline,
        InlineTagKind::Strike,
        InlineTagKind::Code,
    ]
    .iter()
    .all(|&k| !is_armed(k) || group.iter().any(|p| p.kind == k));
    if !inside && !all_armed_in_group {
        return None;
    }
    if inside {
        // Only when the span is the innermost thing around the caret;
        // distinct nested pairs keep the old behavior.
        let nested = line_pair_refs(parsed, caret).iter().any(|p| {
            p.kind != InlineTagKind::Link
                && p.inner != anchor.inner
                && caret >= p.open_delim.start
                && caret <= p.close_delim.end
        });
        if nested {
            return None;
        }
    }

    let has_space = content.as_bytes().get(caret - 1) == Some(&b' ');
    let closer_end = if has_space { caret - 1 } else { caret };
    let group_open_start = span_open_start(parsed, anchor);
    let group_close_end = span_close_end(parsed, anchor);
    // Pairs whose closing markers sit inside the shared run that ends at the
    // caret are re-rendered together with the anchor's group, so their
    // openers are not left dangling by the replacement.
    let mut range_start = group_open_start;
    if !inside && caret > group_close_end {
        let delim = content.as_bytes()[anchor.close_delim.start];
        for p in line_pair_refs(parsed, caret) {
            if p.kind == InlineTagKind::Link || p.inner == anchor.inner {
                continue;
            }
            if p.close_delim.end > group_close_end
                && p.close_delim.end <= caret
                && content[group_close_end..p.close_delim.end]
                    .bytes()
                    .all(|b| b == delim)
            {
                range_start = range_start.min(span_open_start(parsed, p));
            }
        }
    }

    let trail_len = if inside {
        0
    } else {
        text.len() - text.trim_end().len()
    };
    let trail_ws = &text[text.len() - trail_len..];
    let trimmed_text = &text[..text.len() - trail_len];
    let lead_text = if inside {
        trimmed_text.to_string()
    } else if has_space {
        format!(" {}", trimmed_text)
    } else {
        trimmed_text.to_string()
    };

    let in_group = |k: InlineTagKind| group.iter().any(|p| p.kind == k);
    let cont_flags = if inside {
        FormatFlags {
            bold: pending.bold || in_group(InlineTagKind::Bold),
            italic: pending.italic || in_group(InlineTagKind::Italic),
            underline: pending.underline || in_group(InlineTagKind::Underline),
            strike: pending.strike || in_group(InlineTagKind::Strike),
            code: pending.code || in_group(InlineTagKind::Code),
        }
    } else {
        FormatFlags {
            bold: pending.bold,
            italic: pending.italic,
            underline: pending.underline,
            strike: pending.strike,
            code: pending.code,
        }
    };

    let seg_end_limit = if inside { caret } else { closer_end };
    let pline = parsed.line_at_offset(range_start);
    let existing_segs: Vec<&StyledSegment> = pline
        .map(|l| {
            l.styled_segments
                .iter()
                .filter(|s| s.source.start >= range_start && s.source.end <= seg_end_limit)
                .collect()
        })
        .unwrap_or_default();

    let mut slices: Vec<StyledSlice> = if !existing_segs.is_empty() {
        existing_segs
            .iter()
            .map(|s| {
                let raw = segment_raw_range(content, &s.source);
                StyledSlice {
                    text: content[raw].to_string(),
                    flags: FormatFlags {
                        bold: s.bold,
                        italic: s.italic,
                        underline: s.underline,
                        strike: s.strike,
                        code: s.code,
                    },
                    is_selected: false,
                }
            })
            .collect()
    } else {
        let anchor_flags = FormatFlags {
            bold: in_group(InlineTagKind::Bold),
            italic: in_group(InlineTagKind::Italic),
            underline: in_group(InlineTagKind::Underline),
            strike: in_group(InlineTagKind::Strike),
            code: in_group(InlineTagKind::Code),
        };
        vec![StyledSlice {
            text: content[anchor.inner.clone()].to_string(),
            flags: anchor_flags,
            is_selected: false,
        }]
    };

    if !lead_text.is_empty() {
        slices.push(StyledSlice {
            text: lead_text,
            flags: cont_flags,
            is_selected: true,
        });
    }

    let mut sel = None;
    let rendered = render_styled_slices(&slices, &mut sel, 0);
    let repl = format!("{}{}", rendered, trail_ws);
    let replace_end = if inside { group_close_end } else { caret };
    let new_caret = if trail_len > 0 {
        range_start + repl.len()
    } else if let Some(s) = sel {
        range_start + s.end
    } else {
        range_start + repl.len()
    };
    Some((range_start..replace_end, repl, new_caret))
}

/// Inline formats that stay armed at `cursor`, from every marker pair around
/// it. Used to keep typing inside an edited span and to carry formats across
/// a line break.
pub(super) fn pending_formats_at(parsed: &ParsedDocument, cursor: usize) -> ActiveFormats {
    let mut fmt = ActiveFormats::default();
    for pair in line_pair_refs(parsed, cursor) {
        if cursor >= pair.open_delim.start && cursor <= pair.close_delim.end {
            match pair.kind {
                InlineTagKind::Bold => fmt.bold = true,
                InlineTagKind::Italic => fmt.italic = true,
                InlineTagKind::Underline => fmt.underline = true,
                InlineTagKind::Strike => fmt.strike = true,
                InlineTagKind::Code => fmt.code = true,
                InlineTagKind::Link => {}
            }
        }
    }
    fmt
}

pub(super) fn compute_active_formats(
    parsed: &ParsedDocument,
    cursor: usize,
    pending: &ActiveFormats,
) -> ActiveFormats {
    let mut fmt = *pending;
    if let Some(line) = parsed.line_at_offset(cursor) {
        match line.kind {
            LineKind::Heading { level: 1 } => fmt.h1 = true,
            LineKind::Heading { level: 2 } => fmt.h2 = true,
            LineKind::Heading { level: 3 } => fmt.h3 = true,
            LineKind::Heading { level: 4 } => fmt.h4 = true,
            LineKind::Heading { level: 5 } => fmt.h5 = true,
            LineKind::Heading { level: 6.. } => fmt.h6 = true,
            LineKind::ListItem { ordered, task, .. } => {
                if task.is_some() {
                    fmt.checklist = true;
                } else if ordered {
                    fmt.ol = true;
                } else {
                    fmt.ul = true;
                }
            }
            LineKind::Blockquote => fmt.quote = true,
            LineKind::CodeFence { .. } | LineKind::CodeLine => fmt.code = true,
            LineKind::TableRow { .. } | LineKind::TableDelimiter => fmt.table = true,
            _ => {}
        }
        for pair in &line.inline_pairs {
            if cursor >= pair.open_delim.start && cursor <= pair.close_delim.end {
                match pair.kind {
                    InlineTagKind::Bold => fmt.bold = true,
                    InlineTagKind::Italic => fmt.italic = true,
                    InlineTagKind::Underline => fmt.underline = true,
                    InlineTagKind::Strike => fmt.strike = true,
                    InlineTagKind::Code => fmt.code = true,
                    InlineTagKind::Link => fmt.link = true,
                }
            }
        }
    }
    fmt
}
