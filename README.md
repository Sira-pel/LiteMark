# LiteMark

**LiteMark** is an ultra-fast, native Linux Markdown and plain text editor with an authentic Ubuntu Yaru visual aesthetic. Built from the ground up using **Rust** and **GPUI** with **Vulkan hardware acceleration**, LiteMark delivers instantaneous sub-30ms startup times, minimal resource footprint, and fluid 60+ FPS text editing.

---

## ✨ Features

- **Blazing Fast Native Performance**: Pure Rust desktop application using the GPUI modern UI framework with GPU/Vulkan acceleration.
- **Instant WYSIWYG & Markdown**: Live preview and editor rendering headers, lists, code blocks, blockquotes, checklists, and tables.
- **Dedicated Notes Directory**: Automatically tracks and organizes documents in `~/Documents/Notes` with a built-in collapsible file tree sidebar.
- **Ubuntu Yaru Design**: Native Ubuntu styling with seamless switching between Dark (`#300A24`, `#1E1E1E`, Aubergine/Orange `#E95420`) and Light modes.
- **Find & Replace**: In-editor search and replace bar with match counters and navigation.
- **PDF Import**: Pure-Rust PDF text extraction using `lopdf`.
- **Keyboard Shortcuts**: Comprehensive GNOME-style shortcuts (`Ctrl+N`, `Ctrl+O`, `Ctrl+S`, `Ctrl+B`, `Ctrl+I`, `Ctrl+F`, `Ctrl+H`, `Ctrl+\`, etc.).
- **Debian Packaging**: Ready-to-install `.deb` package generation with desktop launcher and scalable icon integration.

---

## 🚀 Getting Started

### Prerequisites

Ensure you have Rust and standard Linux build dependencies installed on Ubuntu/Debian:

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install required system packages
sudo apt update
sudo apt install -y build-essential pkg-config libxkbcommon-dev libxkbcommon-x11-dev libvulkan1 vulkan-tools
```

### Running from Source

```bash
# Debug run
cargo run

# Optimized release run
cargo run --release
```

---

## 📦 Building Debian Package (.deb)

LiteMark includes a script to build a standalone `.deb` package for Ubuntu:

```bash
bash scripts/build-deb.sh
```

The resulting package will be in `build/litemark_1.0.0_amd64.deb`. You can install it on your system with:

```bash
sudo apt install ./build/litemark_1.0.0_amd64.deb
```

Once installed, search for **LiteMark** in your Ubuntu Super / Applications menu or launch `litemark` from the terminal.

---

## ⌨️ Shortcuts Reference

| Shortcut | Action |
| :--- | :--- |
| `Ctrl + N` | New File |
| `Ctrl + O` | Open File Dialog |
| `Ctrl + S` | Save File |
| `Ctrl + \` | Toggle Notes Sidebar |
| `Ctrl + F` | Find in Document |
| `Ctrl + H` | Find & Replace |
| `Ctrl + B` | Bold |
| `Ctrl + I` | Italic |
| `Ctrl + K` | Insert Link |
| `Ctrl + Shift + X` | Strikethrough |
| `Ctrl + Shift + C` | Code Block |
| `Ctrl + Shift + D` | Toggle Dark / Light Theme |
| `F11` | Toggle Fullscreen |

---

## 📄 License

MIT License. See [Cargo.toml](Cargo.toml) for details.
