//! Find and replace.

use super::*;

impl LiteMarkApp {
    pub(super) fn refresh_find(&mut self) {
        if !self.show_find_replace {
            // Don't search a hidden bar; reopening re-runs it.
            self.find_hits.clear();
            self.find_index = None;
            self.not_found = false;
            return;
        }
        self.find_hits = self
            .active_tab()
            .map(|tab| tab.find_hit_ranges(&self.find_term, self.match_case))
            .unwrap_or_default();
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
        if self.find_term.is_empty() {
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
        if self.active_tab().is_some_and(|tab| tab.is_read_only()) {
            self.show_error("PDF files are read-only.");
            return;
        }
        let term = self.find_term.clone();
        let replacement = self.replace_term.clone();
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
