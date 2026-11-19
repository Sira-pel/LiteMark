//! The Ctrl+P quick-open pop-up: find a note by name, or a line inside notes.

use super::*;
use crate::model::quick_open::*;
use std::sync::atomic::Ordering;

/// Wait this long after the last keystroke before searching note contents.
const CONTENT_SEARCH_DEBOUNCE_MS: u64 = 150;

/// What the pop-up shows while it is open.
pub(crate) struct QuickOpen {
    pub mode: QuickMode,
    pub query: String,
    pub selected: usize,
    pub notes: Vec<NoteHit>,
    pub contents: Vec<ContentHit>,
    /// A content search is waiting or running for the current query.
    pub searching: bool,
    pub scroll: ScrollHandle,
    /// Recent notes that still exist, checked on disk once per opening
    /// rather than on every keystroke.
    pub recents: Option<Vec<PathBuf>>,
}

impl QuickOpen {
    pub fn len(&self) -> usize {
        match self.mode {
            QuickMode::Notes => self.notes.len(),
            QuickMode::Contents => self.contents.len(),
        }
    }
}

impl LiteMarkApp {
    /// Ctrl+P and Ctrl+Shift+F. When the pop-up is already open this only
    /// switches its mode.
    pub(crate) fn open_quick_open(&mut self, mode: QuickMode, cx: &mut Context<Self>) {
        if let Some(open) = self.quick_open.as_mut() {
            open.mode = mode;
            open.selected = 0;
            self.refresh_quick_open(cx);
            return;
        }
        // Another pop-up is up (a prompt, Help): leave it be.
        if self.modal_choice_count() > 0 {
            return;
        }
        self.open_menu = None;
        self.menu_selected_ix = None;
        self.menu_open_submenu_ix = None;
        self.editor_context_menu = None;
        self.workspace_menu_open = false;
        self.sidebar_menu = None;
        self.sidebar_keyboard = false;
        self.quick_open_field
            .update(cx, |field, _cx| field.set_text(""));
        self.quick_open = Some(QuickOpen {
            mode,
            query: String::new(),
            selected: 0,
            notes: Vec::new(),
            contents: Vec::new(),
            searching: false,
            scroll: ScrollHandle::new(),
            recents: None,
        });
        self.field_focus = TextInput::QuickOpen;
        self.field_owns_keys = true;
        self.applied_focus = None;
        self.refresh_quick_open(cx);
    }

    pub(crate) fn close_quick_open(&mut self, cx: &mut Context<Self>) {
        if self.quick_open.take().is_none() {
            return;
        }
        // Whatever search is still running is stale now.
        self.quick_open_epoch.fetch_add(1, Ordering::SeqCst);
        self.quick_open_field
            .update(cx, |field, _cx| field.set_text(""));
        self.field_focus = TextInput::Editor;
        self.field_owns_keys = false;
        self.applied_focus = None;
    }

    pub(crate) fn quick_open_switch_mode(&mut self, cx: &mut Context<Self>) {
        let Some(open) = self.quick_open.as_mut() else {
            return;
        };
        open.mode = open.mode.toggled();
        open.selected = 0;
        self.refresh_quick_open(cx);
    }

    pub(crate) fn quick_open_set_mode(&mut self, mode: QuickMode, cx: &mut Context<Self>) {
        if self
            .quick_open
            .as_ref()
            .is_some_and(|open| open.mode != mode)
        {
            self.quick_open_switch_mode(cx);
        }
    }

    pub(crate) fn quick_open_step(&mut self, down: bool) {
        let Some(open) = self.quick_open.as_mut() else {
            return;
        };
        open.selected = step_wrapping(open.selected, open.len(), down);
        open.scroll.scroll_to_item(open.selected);
    }

    pub(crate) fn quick_open_query_changed(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(open) = self.quick_open.as_mut() else {
            return;
        };
        if open.query == text {
            return;
        }
        open.query = text;
        open.selected = 0;
        self.refresh_quick_open(cx);
    }

    /// Recompute the list for the current mode and query. Notes are matched
    /// on the spot from the scanned workspace; contents are searched in the
    /// background after a short pause.
    fn refresh_quick_open(&mut self, cx: &mut Context<Self>) {
        let Some(open) = self.quick_open.as_ref() else {
            return;
        };
        let (mode, query) = (open.mode, open.query.clone());
        match mode {
            QuickMode::Notes => {
                self.quick_open_epoch.fetch_add(1, Ordering::SeqCst);
                let recents = if !query.trim().is_empty() {
                    Vec::new()
                } else if let Some(cached) = open.recents.clone() {
                    cached
                } else {
                    let list = self.recent_notes_for_menu();
                    if let Some(open) = self.quick_open.as_mut() {
                        open.recents = Some(list.clone());
                    }
                    list
                };
                let files: Vec<PathBuf> = self
                    .workspace
                    .files
                    .iter()
                    .map(|file| file.path.clone())
                    .collect();
                let notes = match_notes(
                    &files,
                    &recents,
                    &self.workspace.folder_path,
                    &query,
                    MAX_NOTE_RESULTS,
                );
                if let Some(open) = self.quick_open.as_mut() {
                    open.notes = notes;
                    open.searching = false;
                    open.scroll.scroll_to_item(0);
                }
            }
            QuickMode::Contents => self.start_content_search(query, cx),
        }
        cx.notify();
    }

    fn start_content_search(&mut self, query: String, cx: &mut Context<Self>) {
        let ticket = self.quick_open_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        let epoch = self.quick_open_epoch.clone();
        if query.trim().is_empty() {
            if let Some(open) = self.quick_open.as_mut() {
                open.contents.clear();
                open.searching = false;
            }
            return;
        }
        if let Some(open) = self.quick_open.as_mut() {
            open.searching = true;
        }
        let files: Vec<PathBuf> = self
            .workspace
            .files
            .iter()
            .filter(|file| file.file_type != FileType::Pdf)
            .map(|file| file.path.clone())
            .collect();
        let root = self.workspace.folder_path.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(CONTENT_SEARCH_DEBOUNCE_MS))
                .await;
            if epoch.load(Ordering::SeqCst) != ticket {
                return;
            }
            let search_query = query.clone();
            let stale = epoch.clone();
            let hits = cx
                .background_executor()
                .spawn(async move {
                    search_contents(
                        &files,
                        &root,
                        &search_query,
                        read_searchable,
                        || stale.load(Ordering::SeqCst) != ticket,
                        MAX_HITS_PER_NOTE,
                        MAX_CONTENT_HITS,
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                // A newer query, or a closed pop-up, makes these stale.
                if epoch.load(Ordering::SeqCst) != ticket {
                    return;
                }
                if let Some(open) = this.quick_open.as_mut() {
                    if open.mode == QuickMode::Contents && open.query == query {
                        open.contents = hits;
                        open.searching = false;
                        open.selected = 0;
                        open.scroll.scroll_to_item(0);
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// Enter, or a click on row `ix`.
    pub(crate) fn quick_open_activate(&mut self, ix: Option<usize>, cx: &mut Context<Self>) {
        let Some(open) = self.quick_open.as_ref() else {
            return;
        };
        let ix = ix.unwrap_or(open.selected);
        match open.mode {
            QuickMode::Notes => {
                let Some(path) = open.notes.get(ix).map(|hit| hit.path.clone()) else {
                    return;
                };
                self.close_quick_open(cx);
                self.open_recent(path, cx);
            }
            QuickMode::Contents => {
                let Some(hit) = open.contents.get(ix).cloned() else {
                    return;
                };
                let query = open.query.clone();
                self.close_quick_open(cx);
                self.open_content_hit(hit, query, cx);
            }
        }
        cx.notify();
    }

    /// Open the note, put the caret on the match and load the query into
    /// the find bar, so Enter there steps to the next match.
    pub(crate) fn open_content_hit(
        &mut self,
        hit: ContentHit,
        query: String,
        cx: &mut Context<Self>,
    ) {
        self.open_recent(hit.path.clone(), cx);
        let opened = self
            .active_tab()
            .is_some_and(|tab| tab.file_path.as_deref() == Some(hit.path.as_path()));
        if !opened {
            return;
        }
        self.find_term = query.clone();
        // The content search ignores case, so the find bar must too, or the
        // hit it found may not be among the find bar's matches.
        self.match_case = false;
        self.find_field
            .update(cx, |field, _cx| field.set_text(query));
        self.show_find_replace = true;
        self.show_replace_row = false;
        self.pending_find_focus = true;
        self.find_index = None;
        self.refresh_find();
        let hits = self.find_hits.clone();
        let Some(tab) = self.active_tab_mut() else {
            return;
        };
        let line_start = line_start_offset(&tab.content, hit.line);
        let start = (line_start + hit.col).min(tab.content.len());
        let line_end = tab.content[line_start..]
            .find('\n')
            .map_or(tab.content.len(), |i| line_start + i);
        let index = hits
            .iter()
            .position(|range| range.start == start)
            // The tab text can differ from the file (line endings); fall
            // back to the first match on that line, never a later one.
            .or_else(|| {
                hits.iter()
                    .position(|range| range.start >= line_start && range.start < line_end)
            });
        if let Some(index) = index {
            tab.caret.range = hits[index].clone();
            tab.caret.reversed = false;
            tab.caret.preferred_col = None;
            tab.refresh_cursor();
            self.find_index = Some(index);
            self.not_found = false;
            self.scroll_selection_into_view();
        }
    }
}

/// A note's text for the content search, or `None` for one that is too big,
/// binary or unreadable.
fn read_searchable(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_SEARCH_BYTES {
        return None;
    }
    read_note(path).ok()
}
