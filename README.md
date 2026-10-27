# LiteMark

LiteMark is a native Markdown and text editor for Linux. It edits Markdown and plain text in a raw view or a WYSIWYG view, and stores notes in a folder (by default `~/Documents/Notes`).

## Features

- Raw Markdown and a WYSIWYG view of the same document. Line numbers are available in the raw view.
- Headings, bold, italic, underline, strikethrough, links, lists, task lists, quotes, inline code, and code blocks.
- Tables: a size picker, and a right-click menu to add or delete rows and columns and to set column alignment.
- Find and replace, with match case.
- A notes sidebar for creating notes and folders, renaming, and moving items to the trash. Trashing a note with unsaved edits asks first.
- Open `.md`, `.txt`, and `.pdf` files. A PDF is opened as extracted text.
- Export the current document as HTML, Markdown, or plain text.
- Saves write a temporary file and rename it into place. Saving a new path does not replace an existing file.
- Tabs, undo and redo, zoom, word wrap, centered or left-aligned text, dark / light / system theme, and zen mode.
- Settings are stored in the user config directory as `litemark/settings.json`.

## Updates and network use

LiteMark works fully offline. The only network request it makes is the update check. It is on by default, so the app contacts GitHub (api.github.com) on startup unless you turn it off:

- On startup, in the background, it asks the GitHub Releases API for the latest version. It checks at most once every 24 hours.
- If a newer version exists, a small notice appears with a Download button that opens the release page. Closing the notice hides it for that version.
- Nothing is downloaded or installed automatically, and no notes, settings, or identifiers are sent.
- If you are offline, or GitHub cannot be reached, nothing is shown.
- Turn it off in the settings menu (gear icon) by unchecking "Check for updates", or set `"check_for_updates": false` in `settings.json`.

## Screenshots

_Placeholder. Screenshots are not in the repository yet._

## Install from a .deb

Download the latest `litemark_<version>_amd64.deb` from the [Releases page](https://github.com/Sira-pel/LiteMark/releases/latest), then install it:

```bash
sudo apt install ./litemark_<version>_amd64.deb
```

To update, download and install the newer `.deb` the same way. Your notes and settings are kept.

### Build the .deb yourself

The packaging script builds an amd64 package. `dpkg-dev` must be installed so the script can run `dpkg-shlibdeps`. The version comes from `Cargo.toml`.

```bash
sudo apt install -y dpkg-dev
bash scripts/build-deb.sh
sudo apt install ./build/litemark_*_amd64.deb
```

The package recommends `xdg-desktop-portal` and a GTK or GNOME portal backend. Open and save dialogs use that portal.

## Build from source

Rust 1.85 or newer (stable). Dependencies in the lockfile use the 2024 edition, so an older toolchain will not compile this tree.

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev libfreetype6-dev
cargo build
cargo test
cargo run --release
```

`libxkbcommon-x11-dev` is required at link time. It provides the unversioned `libxkbcommon-x11.so`.

## Runtime

- A GPU with a Vulkan driver (`libvulkan1`).
- X11 or Wayland (`libwayland-client0` on Wayland).
- `libxcb1`, `libxkbcommon0`, and `libxkbcommon-x11-0`.
- Fontconfig (`libfontconfig1`) and FreeType (`libfreetype6`).
- `xdg-desktop-portal` and either `xdg-desktop-portal-gnome` or `xdg-desktop-portal-gtk` for file dialogs.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| Ctrl+N | New tab |
| Ctrl+O | Open a file |
| Ctrl+S | Save |
| Ctrl+Shift+S | Save as |
| Ctrl+W | Close the active tab |
| Ctrl+F | Find |
| Ctrl+H | Find and replace |
| Ctrl+B / I / U | Bold / italic / underline |
| Ctrl+Shift+X | Strikethrough |
| Ctrl+1 / 2 / 3 | Heading 1 / 2 / 3 |
| Ctrl+E | Inline code |
| Ctrl+K | Insert a link |
| Ctrl+Shift+C | Code block |
| Ctrl+Shift+M | Toggle raw and WYSIWYG |
| Ctrl+Shift+L | Toggle line numbers (raw view) |
| Ctrl+Z | Undo |
| Ctrl+Y or Ctrl+Shift+Z | Redo |
| Ctrl+\ | Toggle the notes sidebar |
| Ctrl+= / Ctrl+- / Ctrl+0 | Zoom in / out / reset |
| F1 | Shortcuts |
| F11 | Zen mode |
| Escape | Leave zen mode, or close a dialog |

## Limitations

- Linux only for now.
- PDF export is not implemented.
- Opening a PDF extracts its text. The original pages are not drawn.
- `scripts/build-deb.sh` produces an amd64 package.
- Updates are not installed automatically; the app only tells you when a new release is out.
- Packages for Windows and macOS are not available yet.

## Releasing

1. Set `version` in `Cargo.toml` and commit it.
2. Tag the commit and push the tag:

```bash
git tag v1.0.1
git push origin v1.0.1
```

The [release workflow](.github/workflows/release.yml) checks that the tag matches `Cargo.toml`, builds the `.deb`, and publishes a GitHub Release with the package and a SHA-256 checksum attached.

## License

MIT. See [Cargo.toml](Cargo.toml).
