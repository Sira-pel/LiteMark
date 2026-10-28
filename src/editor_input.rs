//! Bound editor actions that can change a note.
//!
//! `LiteMarkApp` dispatches every mutating editor action through
//! [`dispatch_editor_input`] so a table-drag mouse press blocks edits even
//! after a modifier has already cancelled the visual drag.

use crate::model::{
    command_edits_document, document_accepts_keystrokes, DocumentTab, TextInput, ViewMode,
};
use crate::ui::table_chrome::TableDrag;

/// Bound keys and chords the editor may receive during a table drag.
#[derive(Clone, Copy, Debug)]
pub(crate) enum EditorInput {
    Backspace,
    Delete,
    Newline,
    PlainNewline,
    TableRowBelow,
    Indent,
    Outdent,
    DeleteWordLeft,
    DeleteWordRight,
    DeleteLineStart,
    DeleteLineEnd,
    MoveLineUp,
    MoveLineDown,
    Format(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DispatchOutcome {
    Swallowed,
    Applied,
    Ignored,
    ReadOnly,
}

/// Cancel an active drag and/or swallow input while the originating mouse
/// press is still held. Returns true when the caller must not edit.
pub(crate) fn swallow_table_drag_input(drag: &mut Option<TableDrag>, press_held: bool) -> bool {
    let cancelled = drag.take().is_some();
    cancelled || press_held
}

pub(crate) struct EditorInputState<'a> {
    pub tab: &'a mut DocumentTab,
    pub drag: &'a mut Option<TableDrag>,
    pub press_held: bool,
    pub field_focus: TextInput,
    pub field_owns_keys: bool,
    pub view_mode: ViewMode,
    pub input_blocked: bool,
}

/// Apply one bound editor action, or swallow it.
pub(crate) fn dispatch_editor_input(
    state: &mut EditorInputState<'_>,
    input: EditorInput,
) -> DispatchOutcome {
    if swallow_table_drag_input(state.drag, state.press_held) {
        return DispatchOutcome::Swallowed;
    }
    if state.input_blocked {
        return DispatchOutcome::Ignored;
    }
    let tab = &mut *state.tab;
    let field_focus = state.field_focus;
    let field_owns_keys = state.field_owns_keys;
    let view_mode = state.view_mode;
    match input {
        EditorInput::Format(cmd) => {
            if !command_edits_document(cmd, field_focus, field_owns_keys) {
                return DispatchOutcome::Ignored;
            }
            if tab.is_read_only() && cmd != "undo" && cmd != "redo" {
                return DispatchOutcome::ReadOnly;
            }
            match cmd {
                "undo" => {
                    tab.undo();
                }
                "redo" => {
                    tab.redo();
                }
                other => tab.apply_format_in_mode(other, view_mode),
            }
            DispatchOutcome::Applied
        }
        EditorInput::Backspace => apply_key_edit(tab, field_focus, field_owns_keys, |tab| {
            tab.smart_backspace_in_mode(view_mode);
        }),
        EditorInput::Delete => apply_key_edit(tab, field_focus, field_owns_keys, |tab| {
            tab.delete_forward_in_mode(view_mode);
        }),
        EditorInput::Newline => apply_key_edit(tab, field_focus, field_owns_keys, |tab| {
            tab.smart_newline(false);
        }),
        EditorInput::PlainNewline => apply_key_edit(tab, field_focus, field_owns_keys, |tab| {
            tab.smart_newline(true);
        }),
        EditorInput::TableRowBelow => apply_key_edit(tab, field_focus, field_owns_keys, |tab| {
            tab.table_insert_row_below();
        }),
        EditorInput::Indent => apply_key_edit(tab, field_focus, field_owns_keys, |tab| {
            if !tab.tab_table_cell(false) {
                tab.indent();
            }
        }),
        EditorInput::Outdent => apply_key_edit(tab, field_focus, field_owns_keys, |tab| {
            if !tab.tab_table_cell(true) {
                tab.outdent();
            }
        }),
        EditorInput::DeleteWordLeft => apply_key_edit(tab, field_focus, field_owns_keys, |tab| {
            tab.delete_word_left();
        }),
        EditorInput::DeleteWordRight => apply_key_edit(tab, field_focus, field_owns_keys, |tab| {
            tab.delete_word_right();
        }),
        EditorInput::DeleteLineStart => apply_key_edit(tab, field_focus, field_owns_keys, |tab| {
            tab.delete_to_line_start();
        }),
        EditorInput::DeleteLineEnd => apply_key_edit(tab, field_focus, field_owns_keys, |tab| {
            tab.delete_to_line_end();
        }),
        EditorInput::MoveLineUp => apply_key_edit(tab, field_focus, field_owns_keys, |tab| {
            tab.move_line_up();
        }),
        EditorInput::MoveLineDown => apply_key_edit(tab, field_focus, field_owns_keys, |tab| {
            tab.move_line_down();
        }),
    }
}

fn apply_key_edit(
    tab: &mut DocumentTab,
    field_focus: TextInput,
    field_owns_keys: bool,
    edit: impl FnOnce(&mut DocumentTab),
) -> DispatchOutcome {
    if !document_accepts_keystrokes(field_focus) || field_owns_keys {
        return DispatchOutcome::Ignored;
    }
    if tab.is_read_only() {
        return DispatchOutcome::ReadOnly;
    }
    edit(tab);
    DispatchOutcome::Applied
}
