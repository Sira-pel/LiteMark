#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

VERSION="1.0.0"
PACKAGE_NAME="litemark"
ARCH="amd64"
STAGING_DIR="$ROOT_DIR/build/deb-staging"
OUTPUT_DIR="$ROOT_DIR/build"
DEB_FILE="$OUTPUT_DIR/${PACKAGE_NAME}_${VERSION}_${ARCH}.deb"

echo "🦀 Building Native LiteMark (Rust + GPUI Vulkan Engine)..."
cd "$ROOT_DIR"
cargo build --release

RELEASE_BIN="$ROOT_DIR/target/release/litemark"
if [ ! -f "$RELEASE_BIN" ]; then
    echo "❌ Error: Native release binary not found at $RELEASE_BIN"
    exit 1
fi

echo "🧹 Preparing Debian staging directory at $STAGING_DIR..."
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR/DEBIAN"
mkdir -p "$STAGING_DIR/usr/bin"
mkdir -p "$STAGING_DIR/usr/share/applications"
mkdir -p "$STAGING_DIR/usr/share/icons"
mkdir -p "$OUTPUT_DIR"

# 1. Copy native binary to /usr/bin/litemark
echo "📦 Installing native binary..."
cp "$RELEASE_BIN" "$STAGING_DIR/usr/bin/litemark"
chmod 755 "$STAGING_DIR/usr/bin/litemark"

# 2. Install the hicolor icon theme (16–512 PNG, scalable, and symbolic)
echo "🎨 Installing application icons..."
cp -a "$ROOT_DIR/assets/icons/hicolor" "$STAGING_DIR/usr/share/icons/hicolor"
find "$STAGING_DIR/usr/share/icons/hicolor" -type d -exec chmod 755 {} +
find "$STAGING_DIR/usr/share/icons/hicolor" -type f -exec chmod 644 {} +

# 3. Create desktop launcher
echo "🖥️ Generating desktop launcher..."
cat << 'EOF' > "$STAGING_DIR/usr/share/applications/litemark.desktop"
[Desktop Entry]
Version=1.0
Type=Application
Name=LiteMark
GenericName=Markdown & Text Editor
Comment=Ultra-fast native GPUI markdown editor with Vulkan hardware acceleration
Exec=litemark %F
Icon=litemark
Terminal=false
Categories=Utility;TextEditor;Development;Office;
StartupNotify=true
StartupWMClass=litemark
MimeType=text/plain;text/markdown;application/pdf;
Keywords=Text;Editor;Markdown;Notes;LiteMark;GPUI;Vulkan;Rust;
EOF
chmod 644 "$STAGING_DIR/usr/share/applications/litemark.desktop"

# 4. Generate Debian control file
INSTALLED_SIZE=$(du -sk "$STAGING_DIR" | cut -f1)
echo "📋 Generating package control file..."
cat << EOF > "$STAGING_DIR/DEBIAN/control"
Package: $PACKAGE_NAME
Version: $VERSION
Section: editors
Priority: optional
Architecture: $ARCH
Installed-Size: $INSTALLED_SIZE
Maintainer: LiteMark Team <support@litemark.app>
Depends: libc6, libxcb1, libxkbcommon0, libxkbcommon-x11-0
Description: Ultra-fast native GPUI markdown and text editor for Ubuntu
 LiteMark is a pure native Linux desktop application built with
 Rust and GPUI (Vulkan hardware acceleration). It starts in under
 30ms, features minimal resource footprint, exact Yaru dark and light themes,
 automatic ~/Documents/Notes folder management, instant WYSIWYG
 rendering, pure-Rust PDF extraction, and atomic file saving.
EOF
chmod 644 "$STAGING_DIR/DEBIAN/control"

# 5. Build .deb package
echo "🔨 Building Debian package with dpkg-deb..."
dpkg-deb --build --root-owner-group "$STAGING_DIR" "$DEB_FILE"

echo ""
echo "✅ Native Debian package created successfully:"
echo "   $DEB_FILE ($(du -h "$DEB_FILE" | cut -f1))"
echo ""
echo "📋 Package Inspection:"
dpkg-deb --info "$DEB_FILE"
echo ""
echo "📂 Package Contents:"
dpkg-deb --contents "$DEB_FILE"
