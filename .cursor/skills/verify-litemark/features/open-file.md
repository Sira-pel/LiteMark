# Open a file

Open a file lets a user load an existing markdown or text note into the editor from the File menu, Ctrl+O, or a path passed on the command line.

## Sub-features

- `open-menu` chooses File → Open….
- `open-shortcut` uses Ctrl+O.
- `open-argv` starts the process with one existing path.

## How to get to it (user POV)

- Choose File → Open… in the menu bar.
- Press Ctrl+O.
- Start `litemark` with one path that already exists (`src/main.rs` reads `args[1]`).

## Driving it with control-litemark

Preconditions:

- `scripts/doctor.sh` is not required for this journey. The window must not be started.
- The notes directory is `dirs::document_dir()/Notes` with no override.

- **Refuse the window.** Do not launch the binary. Run `.cursor/skills/verify-litemark/scripts/refuse-gui.sh`. Exit code is 2. Stdout contains the exact line `REFUSED bin=` followed by the debug binary path, and the exact line `command=not-run target/debug/litemark`.
- **Unmet notes directory.** The same command prints `unmet_precondition=notes directory is dirs::document_dir()/Notes with no override (src/services/fs_service.rs get_or_create_notes_dir); launch writes that directory's Welcome.md`.
- **Proof.** Leave the process unstarted. Do not treat a render-dump of the file as this journey.

## Gotchas

- README.md lists Ctrl+O. The binding is `ctrl-o` → `OpenFile` in `src/main.rs`. The menu label is `Open…`.
- A path argument is ignored when the path does not exist (`filter(|p| p.exists())`).
- Launching the binary creates `Welcome.md` under the notes directory when that file is absent. This kit does not isolate that directory.
- `import`, `scrot`, and `grim` are not installed. A screenshot is not available.
