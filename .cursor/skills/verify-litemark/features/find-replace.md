# Find and replace

Find and replace lets a user search the open note, step through matches, and replace one match or every match.

## Sub-features

- `find-menu` opens the bar from Edit → Find.
- `find-shortcut` opens the bar with Ctrl+F.
- `find-replace-one` chooses Replace.
- `find-replace-all` chooses All.

## How to get to it (user POV)

- Choose Edit → Find.
- Press Ctrl+F.
- Press the toolbar button whose tooltip is `Find & Replace (Ctrl+F)`.
- In the bar, choose Previous Match, Next Match, Replace, or All.

## Driving it with control-litemark

Preconditions:

- The find bar is a GPUI element inside the window.
- The window must not be started by this kit.

- **Refuse the window.** Run `.cursor/skills/verify-litemark/scripts/refuse-gui.sh`. Exit code is 2. Stdout contains `command=not-run target/debug/litemark`.
- **Unmet precondition.** Stdout contains `unmet_precondition=notes directory is dirs::document_dir()/Notes with no override (src/services/fs_service.rs get_or_create_notes_dir); launch writes that directory's Welcome.md`.
- **Proof.** Stop. A render dump does not show the find bar's match label.

## Gotchas

- The menu label is `Find` with shortcut `Ctrl+F` (`src/ui/menu_bar.rs`). The binding is `ctrl-f` → `FindReplace`.
- Replace is a button labeled `Replace` (`id` `fr-replace-btn`). Replace-all is a button labeled `All` (`id` `fr-replace-all-btn`).
- Match case is the label `[ ] Aa` or `[x] Aa`, not a separate window.
- GPUI text fields in this app are not driven by xdotool. This kit has no IPC socket.
