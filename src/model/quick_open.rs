//! Quick open (Ctrl+P): fuzzy matching on note names and full-text search
//! inside notes. Pure functions only; the pop-up and the file reads live in
//! the app.

use super::recent_notes::recent_note_label;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// Notes listed for a query, so a one-letter query doesn't build a huge list.
pub const MAX_NOTE_RESULTS: usize = 50;
/// Content hits kept over the whole workspace.
pub const MAX_CONTENT_HITS: usize = 200;
/// Content hits kept from one note, so a note that repeats the query
/// doesn't push every other note out of the list.
pub const MAX_HITS_PER_NOTE: usize = 3;
/// Largest note the content search reads. Bigger files are skipped.
pub const MAX_SEARCH_BYTES: u64 = 4 * 1024 * 1024;
/// How many characters of a matching line are shown.
const SNIPPET_CHARS: usize = 140;
/// How many characters of context to keep before the first match.
const SNIPPET_LEAD_CHARS: usize = 40;

/// Which list the pop-up shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickMode {
    Notes,
    Contents,
}

impl QuickMode {
    pub fn toggled(self) -> Self {
        match self {
            QuickMode::Notes => QuickMode::Contents,
            QuickMode::Contents => QuickMode::Notes,
        }
    }
}

/// A note in the Notes list: what to show and what to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteHit {
    pub path: PathBuf,
    pub name: String,
    pub folder: String,
    /// Byte ranges of `name` the query matched.
    pub name_hits: Vec<Range<usize>>,
    /// Byte ranges of `folder` the query matched (only when the name alone
    /// didn't match).
    pub folder_hits: Vec<Range<usize>>,
}

/// How well a name matched, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Prefix,
    WordStart,
    Subsequence,
    Path,
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn is_word_start(chars: &[(usize, char)], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = chars[i - 1].1;
    let cur = chars[i].1;
    !prev.is_alphanumeric() || (prev.is_lowercase() && cur.is_uppercase())
}

/// Match `query` (already folded) as a subsequence of `text`, starting no
/// earlier than char `from`. Returns the matched char indices. With
/// `boundaries`, after the first char each one is taken right next to the
/// previous one if it fits, else at the next word start, else wherever it
/// first appears.
fn subsequence_from(
    text: &[(usize, char)],
    query: &[char],
    from: usize,
    boundaries: bool,
) -> Option<Vec<usize>> {
    let mut found: Vec<usize> = Vec::with_capacity(query.len());
    let mut pos = from;
    for &q in query {
        let at = |i: usize| fold(text[i].1) == q;
        let mut pick = None;
        if boundaries && !found.is_empty() {
            if pos < text.len() && at(pos) {
                pick = Some(pos);
            } else {
                pick = (pos..text.len()).find(|&i| at(i) && is_word_start(text, i));
            }
        }
        let i = match pick {
            Some(i) => i,
            None => {
                pos + text[pos.min(text.len())..]
                    .iter()
                    .position(|&(_, c)| fold(c) == q)?
            }
        };
        found.push(i);
        pos = i + 1;
    }
    Some(found)
}

/// Merge matched char indices into byte ranges of `text`.
fn ranges_of(text: &[(usize, char)], matched: &[usize]) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for &i in matched {
        let (start, c) = text[i];
        let end = start + c.len_utf8();
        match out.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => out.push(start..end),
        }
    }
    out
}

/// Distance from the first to the last matched char: tighter is better.
fn span(matched: &[usize]) -> usize {
    match (matched.first(), matched.last()) {
        (Some(first), Some(last)) => last - first,
        _ => 0,
    }
}

struct NameMatch {
    tier: Tier,
    span: usize,
    name_hits: Vec<Range<usize>>,
    folder_hits: Vec<Range<usize>>,
}

fn match_name(name: &str, folder: &str, query: &[char]) -> Option<NameMatch> {
    let chars: Vec<(usize, char)> = name.char_indices().collect();
    if chars.len() >= query.len() && chars.iter().zip(query).all(|(&(_, c), &q)| fold(c) == q) {
        let matched: Vec<usize> = (0..query.len()).collect();
        return Some(NameMatch {
            tier: Tier::Prefix,
            span: 0,
            name_hits: ranges_of(&chars, &matched),
            folder_hits: Vec::new(),
        });
    }
    // The tightest match that begins at a word start.
    let word_best = (0..chars.len())
        .filter(|&i| is_word_start(&chars, i) && fold(chars[i].1) == query[0])
        .filter_map(|i| subsequence_from(&chars, query, i, true))
        .min_by_key(|m| span(m));
    if let Some(matched) = word_best {
        return Some(NameMatch {
            tier: Tier::WordStart,
            span: span(&matched),
            name_hits: ranges_of(&chars, &matched),
            folder_hits: Vec::new(),
        });
    }
    if let Some(matched) = subsequence_from(&chars, query, 0, false) {
        return Some(NameMatch {
            tier: Tier::Subsequence,
            span: span(&matched),
            name_hits: ranges_of(&chars, &matched),
            folder_hits: Vec::new(),
        });
    }
    // Last resort: the folder path followed by the name.
    let sep = "/";
    let full = format!("{folder}{sep}{name}");
    let full_chars: Vec<(usize, char)> = full.char_indices().collect();
    let matched = subsequence_from(&full_chars, query, 0, false)?;
    let ranges = ranges_of(&full_chars, &matched);
    let name_start = folder.len() + sep.len();
    let mut name_hits = Vec::new();
    let mut folder_hits = Vec::new();
    for r in ranges {
        if r.start >= name_start {
            name_hits.push(r.start - name_start..r.end - name_start);
        } else {
            folder_hits.push(r.start..r.end.min(folder.len()));
        }
    }
    Some(NameMatch {
        tier: Tier::Path,
        span: span(&matched),
        name_hits,
        folder_hits,
    })
}

/// The Notes list for `query`.
///
/// An empty query lists `recents` (newest first, as stored). Otherwise
/// every note in `files` whose name, or folder and name, contains the query
/// letters in order is listed: names that start with the query first, then
/// those with the match starting at a word, then any other match. Ties go
/// to the tighter match, then the shorter name. `workspace` is the folder
/// the shown folder paths are relative to.
pub fn match_notes(
    files: &[PathBuf],
    recents: &[PathBuf],
    workspace: &Path,
    query: &str,
    limit: usize,
) -> Vec<NoteHit> {
    let query: Vec<char> = query.trim().chars().map(fold).collect();
    if query.is_empty() {
        return recents
            .iter()
            .take(limit)
            .map(|path| {
                let (name, folder) = recent_note_label(path, workspace);
                NoteHit {
                    path: path.clone(),
                    name,
                    folder,
                    name_hits: Vec::new(),
                    folder_hits: Vec::new(),
                }
            })
            .collect();
    }
    let mut scored: Vec<((Tier, usize, usize, String), NoteHit)> = files
        .iter()
        .filter_map(|path| {
            let (name, folder) = recent_note_label(path, workspace);
            let m = match_name(&name, &folder, &query)?;
            let key = (m.tier, m.span, name.chars().count(), name.to_lowercase());
            Some((
                key,
                NoteHit {
                    path: path.clone(),
                    name,
                    folder,
                    name_hits: m.name_hits,
                    folder_hits: m.folder_hits,
                },
            ))
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.path.cmp(&b.1.path)));
    scored.into_iter().take(limit).map(|(_, hit)| hit).collect()
}

/// A line of a note that contains the query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentHit {
    pub path: PathBuf,
    pub name: String,
    pub folder: String,
    /// 1-based.
    pub line: usize,
    /// Byte offset of the first match in the full line.
    pub col: usize,
    /// Byte length of the first match in the full line.
    pub len: usize,
    /// The line as shown: leading blanks dropped, long lines cut.
    pub snippet: String,
    /// Byte ranges of `snippet` the query matched.
    pub snippet_hits: Vec<Range<usize>>,
}

/// One line cut down for display, with the matches moved to fit it.
fn snippet(line: &str, hits: &[Range<usize>]) -> (String, Vec<Range<usize>>) {
    let line = line.trim_end_matches(['\r', '\n']);
    let first = hits.first().map_or(0, |h| h.start);
    // Drop leading blanks, then (for a long line) text before the match.
    let mut start = line.len() - line.trim_start().len();
    start = start.min(first);
    let lead: Vec<usize> = line[start..first]
        .char_indices()
        .map(|(i, _)| start + i)
        .collect();
    let mut cut_front = false;
    if lead.len() > SNIPPET_LEAD_CHARS {
        start = lead[lead.len() - SNIPPET_LEAD_CHARS];
        cut_front = true;
    }
    let mut end = line.len();
    let mut cut_back = false;
    if let Some((i, _)) = line[start..].char_indices().nth(SNIPPET_CHARS) {
        end = start + i;
        cut_back = true;
    }
    let mut text = String::new();
    if cut_front {
        text.push('…');
    }
    let shift = text.len();
    text.push_str(&line[start..end]);
    if cut_back {
        text.push('…');
    }
    let ranges = hits
        .iter()
        .filter(|h| h.start >= start && h.end <= end)
        .map(|h| h.start - start + shift..h.end - start + shift)
        .collect();
    (text, ranges)
}

/// Lines of `text` that contain `query` (ignoring case), at most `max`.
/// Line number (1-based), first match in the line, shown text and its highlights.
type LineMatch = (usize, Range<usize>, String, Vec<Range<usize>>);

fn lines_matching(text: &str, query: &str, max: usize) -> Vec<LineMatch> {
    let mut out = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let hits = super::text_ops::find_matches(line, query, false);
        let Some(first) = hits.first().cloned() else {
            continue;
        };
        let (shown, ranges) = snippet(line, &hits);
        out.push((index + 1, first, shown, ranges));
        if out.len() >= max {
            break;
        }
    }
    out
}

/// Full-text search over `files`.
///
/// `read` gives a note's text, or `None` for one that can't be searched
/// (binary, too big, unreadable). Notes are searched in the order given; at
/// most `per_note` lines come from one note and `max_total` in all, after
/// which the search stops. `stop` is polled between notes so a superseded
/// search ends early.
pub fn search_contents(
    files: &[PathBuf],
    workspace: &Path,
    query: &str,
    read: impl Fn(&Path) -> Option<String>,
    stop: impl Fn() -> bool,
    per_note: usize,
    max_total: usize,
) -> Vec<ContentHit> {
    let mut out = Vec::new();
    if query.is_empty() {
        return out;
    }
    for path in files {
        if out.len() >= max_total || stop() {
            break;
        }
        let Some(text) = read(path) else {
            continue;
        };
        let room = per_note.min(max_total - out.len());
        let found = lines_matching(&text, query, room);
        if found.is_empty() {
            continue;
        }
        let (name, folder) = recent_note_label(path, workspace);
        for (line, first, snippet, snippet_hits) in found {
            out.push(ContentHit {
                path: path.clone(),
                name: name.clone(),
                folder: folder.clone(),
                line,
                col: first.start,
                len: first.end - first.start,
                snippet,
                snippet_hits,
            });
        }
    }
    out
}

/// Byte offset where 1-based `line` starts in `text` (the end of the text
/// when the note has fewer lines).
pub fn line_start_offset(text: &str, line: usize) -> usize {
    let mut offset = 0;
    for _ in 1..line {
        match text[offset..].find('\n') {
            Some(i) => offset += i + 1,
            None => return text.len(),
        }
    }
    offset
}

/// Step a list position by one, wrapping at both ends.
pub fn step_wrapping(current: usize, len: usize, down: bool) -> usize {
    if len == 0 {
        0
    } else if down {
        (current + 1) % len
    } else {
        (current + len - 1) % len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/ws")
    }

    fn files(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(|n| root().join(n)).collect()
    }

    fn names(hits: &[NoteHit]) -> Vec<&str> {
        hits.iter().map(|h| h.name.as_str()).collect()
    }

    #[test]
    fn prefix_beats_word_start_beats_subsequence() {
        let list = files(&[
            "xmeeting.md",
            "Team meeting.md",
            "meeting.md",
            "xmxexexting.md",
        ]);
        let hits = match_notes(&list, &[], &root(), "meet", 10);
        assert_eq!(
            names(&hits),
            vec![
                "meeting.md",
                "Team meeting.md",
                "xmeeting.md",
                "xmxexexting.md"
            ]
        );
    }

    #[test]
    fn word_start_means_after_a_separator_or_a_case_change() {
        let list = files(&["zzquarterplan.md", "q3-plan.md", "QuarterPlan.md"]);
        let hits = match_notes(&list, &[], &root(), "plan", 10);
        // `QuarterPlan` and `q3-plan` match at a word start; `zzquarterplan`
        // only inside a word.
        assert_eq!(names(&hits)[2], "zzquarterplan.md");
    }

    #[test]
    fn highlight_ranges_cover_the_matched_characters() {
        let list = files(&["Meeting Notes.md"]);
        let hits = match_notes(&list, &[], &root(), "meet", 10);
        assert_eq!(hits[0].name_hits, vec![0..4]);
        let hits = match_notes(&list, &[], &root(), "mn", 10);
        assert_eq!(hits[0].name_hits, vec![0..1, 8..9]);
        // Matching is case-insensitive and ranges are byte offsets.
        let list = files(&["Ünicode.md"]);
        let hits = match_notes(&list, &[], &root(), "ÜNI", 10);
        assert_eq!(hits[0].name_hits, vec![0..4]);
        let hits = match_notes(&list, &[], &root(), "ni", 10);
        assert_eq!(hits[0].name_hits, vec![2..4]);
    }

    #[test]
    fn folder_path_is_the_last_resort() {
        let list = vec![root().join("projects").join("a.md"), root().join("b.md")];
        let hits = match_notes(&list, &[], &root(), "proja", 10);
        assert_eq!(names(&hits), vec!["a.md"]);
        assert_eq!(hits[0].folder, "projects");
        assert_eq!(hits[0].folder_hits, vec![0..4]);
        assert_eq!(hits[0].name_hits, vec![0..1]);
    }

    #[test]
    fn tighter_then_shorter_wins_a_tie() {
        let list = files(&["a-long-name-b.md", "ab-long.md", "a-b.md"]);
        let hits = match_notes(&list, &[], &root(), "ab", 10);
        assert_eq!(
            names(&hits),
            vec!["ab-long.md", "a-b.md", "a-long-name-b.md"]
        );
    }

    #[test]
    fn empty_query_lists_recents_in_order() {
        let list = files(&["a.md", "b.md", "c.md"]);
        let recents = vec![root().join("c.md"), root().join("a.md")];
        let hits = match_notes(&list, &recents, &root(), "  ", 10);
        assert_eq!(names(&hits), vec!["c.md", "a.md"]);
        assert!(hits[0].name_hits.is_empty());
        assert_eq!(hits[0].folder, "ws");
        assert!(match_notes(&list, &[], &root(), "", 10).is_empty());
    }

    #[test]
    fn note_results_are_capped() {
        let names: Vec<String> = (0..80).map(|i| format!("note{i}.md")).collect();
        let list: Vec<PathBuf> = names.iter().map(|n| root().join(n)).collect();
        assert_eq!(
            match_notes(&list, &[], &root(), "note", MAX_NOTE_RESULTS).len(),
            MAX_NOTE_RESULTS
        );
    }

    fn reader(text: &'static str) -> impl Fn(&Path) -> Option<String> {
        move |_| Some(text.to_string())
    }

    #[test]
    fn content_hits_carry_line_numbers_and_highlights() {
        let list = files(&["a.md"]);
        let hits = search_contents(
            &list,
            &root(),
            "beta",
            reader("alpha\n\n  Beta here\nlast BETA beta\n"),
            || false,
            10,
            10,
        );
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].line, 3);
        assert_eq!(hits[0].snippet, "Beta here");
        assert_eq!(hits[0].snippet_hits, vec![0..4]);
        assert_eq!((hits[0].col, hits[0].len), (2, 4));
        assert_eq!(hits[1].line, 4);
        assert_eq!(hits[1].snippet_hits, vec![5..9, 10..14]);
        assert_eq!(hits[1].col, 5);
    }

    #[test]
    fn content_hits_are_capped_per_note_and_in_total() {
        let list = files(&["a.md", "b.md", "c.md"]);
        let many = "hit\nhit\nhit\nhit\nhit\n";
        let hits = search_contents(&list, &root(), "hit", reader(many), || false, 2, 100);
        assert_eq!(hits.len(), 6);
        assert_eq!(hits.iter().filter(|h| h.name == "a.md").count(), 2);
        let hits = search_contents(&list, &root(), "hit", reader(many), || false, 2, 5);
        assert_eq!(hits.len(), 5);
        assert_eq!(hits.last().unwrap().name, "c.md");
    }

    #[test]
    fn unreadable_notes_are_skipped_and_stop_ends_early() {
        let list = files(&["bin.md", "ok.md"]);
        let read = |p: &Path| (p.file_name().unwrap() == "ok.md").then(|| "needle".to_string());
        let hits = search_contents(&list, &root(), "needle", read, || false, 3, 10);
        assert_eq!(names_of(&hits), vec!["ok.md"]);
        let hits = search_contents(&list, &root(), "needle", read, || true, 3, 10);
        assert!(hits.is_empty());
        assert!(search_contents(&list, &root(), "", read, || false, 3, 10).is_empty());
    }

    fn names_of(hits: &[ContentHit]) -> Vec<&str> {
        hits.iter().map(|h| h.name.as_str()).collect()
    }

    #[test]
    fn long_lines_are_cut_around_the_match() {
        let line = format!("{}needle{}", "x".repeat(300), "y".repeat(300));
        let (shown, ranges) = snippet(&line, std::slice::from_ref(&(300..306)));
        assert!(shown.starts_with('…') && shown.ends_with('…'));
        assert_eq!(&shown[ranges[0].clone()], "needle");
        assert!(shown.chars().count() < 200);
    }

    #[test]
    fn line_start_offset_finds_lines() {
        let text = "one\ntwo\nthree";
        assert_eq!(line_start_offset(text, 1), 0);
        assert_eq!(line_start_offset(text, 2), 4);
        assert_eq!(line_start_offset(text, 3), 8);
        assert_eq!(line_start_offset(text, 9), text.len());
    }

    #[test]
    fn stepping_wraps_both_ways() {
        assert_eq!(step_wrapping(2, 3, true), 0);
        assert_eq!(step_wrapping(0, 3, false), 2);
        assert_eq!(step_wrapping(0, 0, true), 0);
    }
}
