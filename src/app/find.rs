//! Find and replace.

use super::*;

impl LiteMarkApp {
    pub(super) fn refresh_find(&mut self) {
        if !self.show_find_replace {
            // Don't search a hidden bar; reopening re-runs it.
            self.find_hits = Vec::new().into();
            self.find_hits_key = None;
            self.find_index = None;
            self.not_found = false;
            return;
        }
        self.find_hits = self
            .active_tab()
            .map(|tab| tab.find_hit_ranges(&self.find_term, self.match_case))
            .unwrap_or_default()
            .into();
        self.find_hits_key = self.active_tab().map(|tab| {
            (
                tab.id.clone(),
                tab.revision,
                self.find_term.clone(),
                self.match_case,
            )
        });
        if self.find_term.is_empty() {
            self.find_index = None;
            self.not_found = false;
        } else if self.find_hits.is_empty() {
            self.find_index = None;
            self.not_found = true;
        } else {
            self.not_found = false;
            if self
                .find_index
                .is_some_and(|index| index >= self.find_hits.len())
            {
                self.find_index = None;
            }
        }
    }

    fn ensure_find_hits(&mut self) {
        let fresh = self.active_tab().is_some_and(|tab| {
            self.find_hits_key
                .as_ref()
                .is_some_and(|(id, rev, term, case)| {
                    *id == tab.id
                        && *rev == tab.revision
                        && *term == self.find_term
                        && *case == self.match_case
                })
        });
        if !fresh {
            self.refresh_find();
        }
    }

    pub(super) fn match_label(&self) -> String {
        if self.find_term.is_empty() {
            String::new()
        } else if self.find_hits.is_empty() {
            "No results".into()
        } else if let Some(index) = self.find_index {
            format!("{} of {}", index + 1, self.find_hits.len())
        } else {
            format!("{} matches", self.find_hits.len())
        }
    }

    pub(super) fn find_next(&mut self) {
        self.step_find(true);
    }

    pub(super) fn find_previous(&mut self) {
        self.step_find(false);
    }

    pub(super) fn step_find(&mut self, forward: bool) {
        // A prompt is open: the find panel's buttons must not act behind it.
        if self.input_blocked() || self.find_term.is_empty() {
            return;
        }
        if self.show_find_replace {
            self.ensure_find_hits();
            let current = self.find_index;
            // A pointer copy: the editor element shares the same hits.
            let hits = self.find_hits.clone();
            let index = self
                .active_tab_mut()
                .and_then(|tab| tab.select_match_in(&hits, current, forward));
            self.find_index = index;
            self.not_found = index.is_none();
            if index.is_some() {
                self.scroll_selection_into_view();
            }
            return;
        }
        let term = self.find_term.clone();
        let case = self.match_case;
        let current = self.find_index;
        let index = self
            .active_tab_mut()
            .and_then(|tab| tab.select_match(&term, case, current, forward));
        self.find_index = index;
        self.refresh_find();
        if index.is_none() {
            self.not_found = true;
        } else {
            self.scroll_selection_into_view();
        }
    }

    pub(super) fn replace_current(&mut self) {
        if self.input_blocked() {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        let replacement = self.replace_term.clone();
        if self.show_find_replace {
            if self.find_term.is_empty() {
                return;
            }
            self.ensure_find_hits();
            let current = self.find_index;
            let hit = self.active_tab().and_then(|tab| {
                let index = current
                    .filter(|index| self.find_hits.get(*index).is_some())
                    .or_else(|| {
                        crate::model::text_ops::find_next(
                            &self.find_hits,
                            tab.cursor_offset(),
                            None,
                        )
                    })?;
                self.find_hits.get(index).cloned()
            });
            let Some(hit) = hit else {
                self.find_index = None;
                self.not_found = true;
                return;
            };
            if let Some(tab) = self.active_tab_mut() {
                tab.edit(hit, &replacement);
            }
            self.refresh_find();
            let hits = self.find_hits.clone();
            let index = self
                .active_tab_mut()
                .and_then(|tab| tab.select_match_in(&hits, None, true));
            self.find_index = index;
            if index.is_some() {
                self.scroll_selection_into_view();
            }
            return;
        }
        let term = self.find_term.clone();
        let case = self.match_case;
        let current = self.find_index;
        let index = self
            .active_tab_mut()
            .and_then(|tab| tab.replace_current_match(&term, &replacement, case, current));
        self.find_index = index;
        self.refresh_find();
        if index.is_some() {
            self.scroll_selection_into_view();
        }
    }

    pub(super) fn replace_all_matches(&mut self) {
        if self.input_blocked() {
            return;
        }
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        let term = self.find_term.clone();
        let replacement = self.replace_term.clone();
        let case = self.match_case;
        let changed = self
            .active_tab_mut()
            .is_some_and(|tab| tab.replace_every_match(&term, &replacement, case));
        self.find_index = None;
        self.refresh_find();
        if !changed && !term.is_empty() {
            self.not_found = true;
        }
    }
}
