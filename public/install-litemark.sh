#!/usr/bin/env bash
# LiteMark - Seamless Ubuntu Desktop Setup Script
set -e

echo "🚀 Installing LiteMark to your system..."

# 1. Create local application and notes directories
mkdir -p "$HOME/Documents/Notes"
mkdir -p "$HOME/.local/share/applications"
mkdir -p "$HOME/.local/share/icons/hicolor/scalable/apps"

# 2. Install LiteMark application icon
cat << 'EOF' > "$HOME/.local/share/icons/hicolor/scalable/apps/litemark.svg"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <defs>
    <linearGradient id="bgGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#2D2034"/>
      <stop offset="45%" stop-color="#1E1E24"/>
      <stop offset="100%" stop-color="#141416"/>
    </linearGradient>
    <linearGradient id="accentGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#FF6B35"/>
      <stop offset="50%" stop-color="#E95420"/>
      <stop offset="100%" stop-color="#C71585"/>
    </linearGradient>
    <linearGradient id="featherGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#FFFFFF"/>
      <stop offset="100%" stop-color="#DFDBD2"/>
    </linearGradient>
  </defs>
  <rect x="28" y="28" width="456" height="456" rx="112" fill="url(#bgGrad)"/>
  <rect x="29" y="29" width="454" height="454" rx="111" fill="none" stroke="rgba(255,255,255,0.12)" stroke-width="2"/>
  <circle cx="256" cy="256" r="160" fill="url(#accentGrad)" opacity="0.16"/>
  <rect x="116" y="96" width="280" height="320" rx="24" fill="#242426" stroke="#38383C" stroke-width="3"/>
  <rect x="116" y="96" width="280" height="12" rx="6" fill="url(#accentGrad)"/>
  <line x1="160" y1="164" x2="352" y2="164" stroke="#44444A" stroke-width="6" stroke-linecap="round"/>
  <line x1="160" y1="204" x2="280" y2="204" stroke="#44444A" stroke-width="6" stroke-linecap="round"/>
  <path d="M 164 252 L 164 360 L 228 360" fill="none" stroke="url(#featherGrad)" stroke-width="18" stroke-linecap="round" stroke-linejoin="round"/>
  <path d="M 248 360 L 272 268 L 308 316 L 344 268 L 368 360" fill="none" stroke="url(#accentGrad)" stroke-width="18" stroke-linecap="round" stroke-linejoin="round"/>
  <polygon points="308,368 296,348 320,348" fill="url(#accentGrad)"/>
</svg>
EOF

# 3. Create .desktop launcher for Ubuntu Applications menu & Dock
APP_URL="https://ais-dev-bcco4o4744w66wxgtdzpr6-871645041052.asia-east1.run.app"

cat << EOF > "$HOME/.local/share/applications/litemark.desktop"
[Desktop Entry]
Version=1.0
Type=Application
Name=LiteMark
Comment=Fast, distraction-free markdown & text editor with dedicated notes folder
Exec=xdg-open "$APP_URL"
Icon=litemark
Terminal=false
Categories=Utility;TextEditor;Development;
StartupNotify=true
StartupWMClass=litemark
EOF

chmod +x "$HOME/.local/share/applications/litemark.desktop"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true

echo "✅ Installed successfully! You can now launch 'LiteMark' from your Ubuntu Applications menu (Super key) or pin it to your Ubuntu Dock."
