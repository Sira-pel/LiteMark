//! Bound editor actions and text input that can change a note.
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
    #[allow(dead_code)]
    InsertText(&'static str),
    #[allow(dead_code)]
    Escape,
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

/// Apply one bound editor action or text insert, or swallow it.
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
        EditorInput::Escape => DispatchOutcome::Ignored,
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
        EditorInput::InsertText(text) => apply_key_edit(tab, field_focus, field_owns_keys, |tab| {
            let range = tab.selection_range();
            tab.insert_text_in_mode(range, text, view_mode);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::table_chrome::{TableDrag, TableDragKind};

    const TABLE: &str = "| A | B |\n| --- | --- |\n| d | e |\n| g | h |\n";

    fn table_doc() -> DocumentTab {
        let mut doc = DocumentTab::new("t".into(), "t.md".into(), TABLE.into(), None, None);
        doc.caret.move_to(26);
        doc
    }

    fn row_drag() -> TableDrag {
        TableDrag {
            source: 0..43,
            kind: TableDragKind::Row,
            from: 1,
            anchor: 26,
            gap: 1,
            drop_visible: true,
        }
    }

    fn run(
        tab: &mut DocumentTab,
        drag: &mut Option<TableDrag>,
        press_held: bool,
        input: EditorInput,
    ) -> DispatchOutcome {
        dispatch_editor_input(
            &mut EditorInputState {
                tab,
                drag,
                press_held,
                field_focus: TextInput::Editor,
                field_owns_keys: false,
                view_mode: ViewMode::Wysiwyg,
                input_blocked: false,
            },
            input,
        )
    }

    fn assert_unchanged(input: EditorInput, press_held: bool, start_with_drag: bool) {
        let mut tab = table_doc();
        let mut drag = if start_with_drag {
            Some(row_drag())
        } else {
            None
        };
        let outcome = run(&mut tab, &mut drag, press_held, input);
        assert_eq!(tab.content, TABLE, "input {input:?} changed bytes");
        assert!(drag.is_none(), "input {input:?} left a drag active");
        assert_eq!(outcome, DispatchOutcome::Swallowed);
    }

    #[test]
    fn bound_keys_do_not_edit_during_a_table_drag() {
        for input in [
            EditorInput::Backspace,
            EditorInput::Delete,
            EditorInput::Newline,
            EditorInput::Indent,
            EditorInput::InsertText("x"),
            EditorInput::Escape,
        ] {
            assert_unchanged(input, false, true);
        }
    }

    #[test]
    fn chords_do_not_edit_while_the_drag_press_is_held() {
        // Modifier already cancelled the visual drag; the mouse is still down.
        for input in [
            EditorInput::PlainNewline,
            EditorInput::TableRowBelow,
            EditorInput::DeleteWordLeft,
            EditorInput::DeleteLineStart,
            EditorInput::DeleteLineEnd,
            EditorInput::MoveLineUp,
            EditorInput::MoveLineDown,
            EditorInput::Format("bold"),
            EditorInput::Format("italic"),
            EditorInput::Format("underline"),
            EditorInput::Format("h1"),
            EditorInput::Format("code"),
            EditorInput::Format("strike"),
            EditorInput::Format("codeblock"),
            EditorInput::Backspace,
            EditorInput::Delete,
            EditorInput::Newline,
            EditorInput::Indent,
            EditorInput::InsertText("x"),
        ] {
            assert_unchanged(input, true, false);
        }
    }

    #[test]
    fn bound_keys_edit_when_no_drag_is_held() {
        let mut tab = table_doc();
        let mut drag = None;
        let before = tab.content.clone();
        let outcome = run(&mut tab, &mut drag, false, EditorInput::InsertText("x"));
        assert_eq!(outcome, DispatchOutcome::Applied);
        assert_ne!(tab.content, before);
    }

    #[test]
    fn app_handlers_dispatch_through_this_module() {
        let src = include_str!("app.rs");
        for needle in [
            "self.dispatch_editor_input(EditorInput::Backspace)",
            "self.dispatch_editor_input(EditorInput::Delete)",
            "self.dispatch_editor_input(EditorInput::Newline)",
            "self.dispatch_editor_input(EditorInput::Indent)",
            "self.dispatch_editor_input(EditorInput::Outdent)",
            "self.dispatch_editor_input(EditorInput::PlainNewline)",
            "self.dispatch_editor_input(EditorInput::TableRowBelow)",
            "self.dispatch_editor_input(EditorInput::DeleteWordLeft)",
            "self.dispatch_editor_input(EditorInput::DeleteWordRight)",
            "self.dispatch_editor_input(EditorInput::DeleteLineStart)",
            "self.dispatch_editor_input(EditorInput::DeleteLineEnd)",
            "dispatch_editor_input(EditorInput::MoveLineUp)",
            "dispatch_editor_input(EditorInput::MoveLineDown)",
            "self.dispatch_editor_input(EditorInput::Format(cmd))",
            "self.block_table_drag_edit()",
        ] {
            assert!(
                src.contains(needle),
                "app.rs lost the drag guard call {needle}"
            );
        }
    }
}
