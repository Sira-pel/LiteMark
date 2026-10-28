#!/usr/bin/env bash
# Builds LiteMark.app and a .dmg for the current Mac's architecture.
# The app is ad-hoc signed, not notarized: on first launch right-click the app and choose Open.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT_DIR/Cargo.toml" | head -n 1)"
case "$(uname -m)" in
    arm64) ARCH="arm64" ;;
    x86_64) ARCH="x64" ;;
    *) ARCH="$(uname -m)" ;;
esac
OUT="$ROOT_DIR/build"
APP="$OUT/LiteMark.app"
DMG="$OUT/LiteMark-${VERSION}-macos-${ARCH}.dmg"

cd "$ROOT_DIR"
cargo build --release

rm -rf "$APP" "$OUT/dmg-staging" "$OUT/LiteMark.iconset"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$OUT/LiteMark.iconset"
cp target/release/litemark "$APP/Contents/MacOS/litemark"

# icns from the hicolor PNGs
H="$ROOT_DIR/assets/icons/hicolor"
I="$OUT/LiteMark.iconset"
cp "$H/16x16/apps/litemark.png"   "$I/icon_16x16.png"
cp "$H/32x32/apps/litemark.png"   "$I/icon_16x16@2x.png"
cp "$H/32x32/apps/litemark.png"   "$I/icon_32x32.png"
cp "$H/64x64/apps/litemark.png"   "$I/icon_32x32@2x.png"
cp "$H/128x128/apps/litemark.png" "$I/icon_128x128.png"
cp "$H/256x256/apps/litemark.png" "$I/icon_128x128@2x.png"
cp "$H/256x256/apps/litemark.png" "$I/icon_256x256.png"
cp "$H/512x512/apps/litemark.png" "$I/icon_256x256@2x.png"
cp "$H/512x512/apps/litemark.png" "$I/icon_512x512.png"
iconutil -c icns "$I" -o "$APP/Contents/Resources/litemark.icns"
rm -rf "$I"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>LiteMark</string>
    <key>CFBundleDisplayName</key><string>LiteMark</string>
    <key>CFBundleIdentifier</key><string>io.github.sira-pel.litemark</string>
    <key>CFBundleExecutable</key><string>litemark</string>
    <key>CFBundleIconFile</key><string>litemark</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundleVersion</key><string>${VERSION}</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key><string>Markdown and text</string>
            <key>CFBundleTypeRole</key><string>Editor</string>
            <key>LSHandlerRank</key><string>Alternate</string>
            <key>CFBundleTypeExtensions</key><array><string>md</string><string>markdown</string><string>txt</string></array>
        </dict>
    </array>
</dict>
</plist>
PLIST

codesign --force --deep --sign - "$APP"

mkdir -p "$OUT/dmg-staging"
cp -R "$APP" "$OUT/dmg-staging/"
ln -s /Applications "$OUT/dmg-staging/Applications"
rm -f "$DMG"
hdiutil create -volname "LiteMark" -srcfolder "$OUT/dmg-staging" -ov -format UDZO "$DMG"
rm -rf "$OUT/dmg-staging"

( cd "$OUT" && shasum -a 256 "$(basename "$DMG")" > "SHA256SUMS-macos-${ARCH}.txt" )
echo "Built $DMG"
