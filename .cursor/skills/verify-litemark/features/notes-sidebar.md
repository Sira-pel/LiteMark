# Notes sidebar

The notes sidebar lists the notes directory and lets a user show or hide that list.

## Sub-features

- `sidebar-menu` toggles View → Sidebar.
- `sidebar-shortcut` toggles Ctrl+\\.
- `sidebar-new-note` shows the New note row.

## How to get to it (user POV)

- Choose View → Sidebar.
- Press Ctrl+\\.
- Choose `New note (Ctrl+N)` in the sidebar.

## Driving it with control-litemark

Preconditions:

- The sidebar reads the notes directory created by `get_or_create_notes_dir`.
- That directory is not isolated. Do not start the window.

- **Refuse the window.** Run `.cursor/skills/verify-litemark/scripts/refuse-gui.sh`. Exit code is 2. Stdout contains `command=not-run target/debug/litemark`.
- **Unmet precondition.** Stdout contains `unmet_precondition=notes directory is dirs::document_dir()/Notes with no override (src/services/fs_service.rs get_or_create_notes_dir); launch writes that directory's Welcome.md`.
- **Proof.** Stop. Do not create `Welcome.md` to force the sidebar to look populated.

## Gotchas

- The menu item label is `Sidebar` with shortcut `Ctrl+\` (`src/ui/menu_bar.rs`). The binding is `ctrl-\` → `ToggleSidebar`.
- First launch writes `Welcome.md` when it is missing (`seed_welcome_note`).
- There is no env var in this repo that moves the notes directory.
