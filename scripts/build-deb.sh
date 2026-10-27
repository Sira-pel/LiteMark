#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT_DIR/Cargo.toml" | head -n 1)"
PACKAGE_NAME="litemark"
ARCH="amd64"
STAGING_DIR="$ROOT_DIR/build/deb-staging"
OUTPUT_DIR="$ROOT_DIR/build"
DEB_FILE="$OUTPUT_DIR/${PACKAGE_NAME}_${VERSION}_${ARCH}.deb"

echo "Building LiteMark..."
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
Comment=Native Markdown and text editor
Exec=litemark %F
Icon=litemark
Terminal=false
Categories=Utility;TextEditor;Development;Office;
StartupNotify=true
StartupWMClass=litemark
MimeType=text/plain;text/markdown;application/pdf;
Keywords=Text;Editor;Markdown;Notes;LiteMark;
EOF
chmod 644 "$STAGING_DIR/usr/share/applications/litemark.desktop"

# 4. Depends from the release binary, plus libraries loaded with dlopen.
# dpkg-shlibdeps sees NEEDED entries (libc, libgcc, xcb, xkbcommon).
# Vulkan, Wayland, fontconfig, and FreeType are not in that list.
echo "Resolving shared library dependencies..."
DLOPEN_PKGS="libvulkan1 libwayland-client0 libfontconfig1 libfreetype6"
FALLBACK_DEPENDS="libc6, libgcc-s1, libxcb1, libxkbcommon0, libxkbcommon-x11-0, libvulkan1, libwayland-client0, libfontconfig1, libfreetype6"
DEPENDS=""
if command -v dpkg-shlibdeps >/dev/null 2>&1; then
  dep_work=$(mktemp -d)
  mkdir -p "$dep_work/debian"
  cat > "$dep_work/debian/control" << EOF
Source: litemark

Package: litemark
Architecture: any
Description: placeholder
 placeholder
EOF
  if shlib_out=$(cd "$dep_work" && dpkg-shlibdeps -O "$STAGING_DIR/usr/bin/litemark" 2>/dev/null); then
    DEPENDS=$(printf '%s\n' "$shlib_out" | sed -n 's/^shlibs:Depends=//p' | head -n 1)
  fi
  rm -rf "$dep_work"
fi
if [ -z "$DEPENDS" ]; then
  echo "dpkg-shlibdeps did not produce Depends; using the known runtime library list."
  DEPENDS="$FALLBACK_DEPENDS"
else
  for pkg in $DLOPEN_PKGS; do
    case ", $DEPENDS," in
      *", $pkg,"*|*", $pkg ("*) ;;
      *) DEPENDS="$DEPENDS, $pkg" ;;
    esac
  done
fi
RECOMMENDS="xdg-desktop-portal, xdg-desktop-portal-gnome | xdg-desktop-portal-gtk"

# 5. Generate Debian control file
INSTALLED_SIZE=$(du -sk "$STAGING_DIR" | cut -f1)
echo "Generating package control file..."
cat << EOF > "$STAGING_DIR/DEBIAN/control"
Package: $PACKAGE_NAME
Version: $VERSION
Section: editors
Priority: optional
Architecture: $ARCH
Installed-Size: $INSTALLED_SIZE
Maintainer: LiteMark Team <support@litemark.app>
Depends: $DEPENDS
Recommends: $RECOMMENDS
Description: Native Markdown and text editor for Linux
 LiteMark edits Markdown and plain text on Linux. It keeps a notes
 folder, and includes raw and WYSIWYG views, find and replace, table
 editing, HTML/Markdown/plain-text export, PDF text import, and atomic saves.
EOF
chmod 644 "$STAGING_DIR/DEBIAN/control"

# 6. Build .deb package
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
