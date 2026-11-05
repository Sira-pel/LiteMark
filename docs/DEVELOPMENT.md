# Development

## Build from source

Requires Rust 1.85 or newer (stable). Dependencies in the lockfile use the 2024 edition, so an older toolchain will not compile this tree.

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev libfreetype6-dev
cargo build
cargo test
cargo run --release
```

`libxkbcommon-x11-dev` is required at link time. It provides the unversioned `libxkbcommon-x11.so`.

## Runtime requirements (Linux)

- A GPU with a Vulkan driver (`libvulkan1`).
- X11 or Wayland (`libwayland-client0` on Wayland).
- `libxcb1`, `libxkbcommon0`, and `libxkbcommon-x11-0`.
- Fontconfig (`libfontconfig1`) and FreeType (`libfreetype6`).
- `xdg-desktop-portal` and either `xdg-desktop-portal-gnome` or `xdg-desktop-portal-gtk` for file dialogs.

## Packaging

- Windows: `powershell scripts/build-windows.ps1` (needs Rust and Inno Setup 6).
- macOS: `bash scripts/build-macos.sh` (needs Rust and Xcode command line tools).
- Linux `.deb` (amd64): `bash scripts/build-deb.sh`. `dpkg-dev` must be installed so the script can run `dpkg-shlibdeps`. The version comes from `Cargo.toml`.

  ```bash
  sudo apt install -y dpkg-dev
  bash scripts/build-deb.sh
  sudo apt install ./build/litemark_*_amd64.deb
  ```

The `.deb` recommends `xdg-desktop-portal` and a GTK or GNOME portal backend, which the open and save dialogs use.

## Releasing

Use plain `0.x.y` versions while in beta. Do not tick GitHub's "pre-release" box: the in-app update check only sees the latest non-pre-release. The update check ignores `-beta` style suffixes, so avoid them in version numbers.

1. Set `version` in `Cargo.toml` and commit it.
2. Tag the commit and push the tag:

   ```bash
   git tag v0.9.1
   git push origin v0.9.1
   ```

The [release workflow](../.github/workflows/release.yml) checks that the tag matches `Cargo.toml`, builds the packages for every platform, and publishes a GitHub Release with the installers and SHA-256 checksums attached. It also uploads fixed-name copies so the README download links always resolve.
