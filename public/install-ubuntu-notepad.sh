#!/usr/bin/env bash
# Ubuntu Notepad - Seamless Desktop Setup Script
set -e

echo "🚀 Installing Ubuntu Notepad to your system..."

# 1. Create local application directories
mkdir -p "$HOME/.local/share/applications"
mkdir -p "$HOME/.local/share/icons/hicolor/scalable/apps"

# 2. Download application icon
cat << 'EOF' > "$HOME/.local/share/icons/hicolor/scalable/apps/ubuntu-notepad.svg"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <defs>
    <linearGradient id="bgGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#E95420"/>
      <stop offset="100%" stop-color="#77216F"/>
    </linearGradient>
  </defs>
  <rect x="24" y="24" width="464" height="464" rx="108" fill="url(#bgGrad)"/>
  <rect x="110" y="96" width="292" height="320" rx="16" fill="#FFFFFF"/>
  <rect x="150" y="160" width="160" height="14" rx="7" fill="#E95420"/>
  <rect x="150" y="200" width="212" height="10" rx="5" fill="#AEA79F"/>
  <rect x="150" y="232" width="190" height="10" rx="5" fill="#AEA79F"/>
  <rect x="150" y="264" width="204" height="10" rx="5" fill="#AEA79F"/>
</svg>
EOF

# 3. Create .desktop launcher for Ubuntu Applications menu & Dock
APP_URL="https://ais-dev-bcco4o4744w66wxgtdzpr6-871645041052.asia-east1.run.app"

cat << EOF > "$HOME/.local/share/applications/ubuntu-notepad.desktop"
[Desktop Entry]
Version=1.0
Type=Application
Name=Ubuntu Notepad
Comment=Fast, distraction-free markdown & text editor
Exec=xdg-open "$APP_URL"
Icon=ubuntu-notepad
Terminal=false
Categories=Utility;TextEditor;Development;
StartupNotify=true
StartupWMClass=ubuntu-notepad
EOF

chmod +x "$HOME/.local/share/applications/ubuntu-notepad.desktop"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true

echo "✅ Installed successfully! You can now launch 'Ubuntu Notepad' from your Ubuntu Applications menu (Super key) or pin it to your Ubuntu Dock."
