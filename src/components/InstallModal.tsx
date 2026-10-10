import React, { useState, useEffect } from 'react';
import {
  Download,
  Terminal,
  Check,
  Copy,
  ExternalLink,
  Laptop,
  Box,
  Layers,
  ChevronRight,
  ShieldCheck,
  Zap,
  FolderDown,
  Monitor,
  Flame,
} from 'lucide-react';
import { ThemeMode } from '../types';

interface InstallModalProps {
  isOpen: boolean;
  onClose: () => void;
  theme: ThemeMode;
  deferredPrompt?: any;
  onTriggerPwaInstall?: () => void;
  isStandalone?: boolean;
}

export function InstallModal({
  isOpen,
  onClose,
  theme,
  deferredPrompt,
  onTriggerPwaInstall,
  isStandalone,
}: InstallModalProps) {
  const [activeTab, setActiveTab] = useState<'pwa' | 'deb' | 'script'>('pwa');
  const [copiedId, setCopiedId] = useState<string | null>(null);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && isOpen) {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const isDark = theme === 'yaru-dark';

  const copyToClipboard = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  const oneClickScript = `#!/usr/bin/env bash
# Ubuntu Notepad - Seamless Desktop Setup Script
set -e

echo "🚀 Installing Ubuntu Notepad to your system..."

# 1. Create local application directories
mkdir -p "$HOME/.local/share/applications"
mkdir -p "$HOME/.local/share/icons/hicolor/scalable/apps"

# 2. Download application icon
cat << 'EOF' > "$HOME/.local/share/icons/hicolor/scalable/apps/ubuntu-notepad.svg"
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <rect x="24" y="24" width="464" height="464" rx="108" fill="#E95420"/>
  <rect x="110" y="96" width="292" height="320" rx="16" fill="#FFFFFF"/>
  <rect x="150" y="160" width="160" height="14" rx="7" fill="#E95420"/>
  <rect x="150" y="200" width="212" height="10" rx="5" fill="#AEA79F"/>
  <rect x="150" y="232" width="190" height="10" rx="5" fill="#AEA79F"/>
  <rect x="150" y="264" width="204" height="10" rx="5" fill="#AEA79F"/>
</svg>
EOF

# 3. Create .desktop launcher for Ubuntu Applications menu & Dock
APP_URL="${typeof window !== 'undefined' ? window.location.href : 'https://ais-dev-bcco4o4744w66wxgtdzpr6-871645041052.asia-east1.run.app'}"

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

echo "✅ Installed successfully! Search 'Ubuntu Notepad' in your Ubuntu Super menu or dock."
`;

  const downloadScriptFile = () => {
    const blob = new Blob([oneClickScript], { type: 'text/x-sh' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'install-ubuntu-notepad.sh';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-3 sm:p-4 bg-black/60 backdrop-blur-xs select-text animate-in fade-in duration-150">
      <div
        className={`w-full max-w-2xl rounded-xl shadow-2xl border flex flex-col max-h-[92vh] overflow-hidden ${
          isDark
            ? 'bg-[#242424] border-[#3e3e3e] text-[#dfdbd2]'
            : 'bg-[#fafafa] border-neutral-300 text-neutral-900'
        }`}
      >
        {/* Header */}
        <div
          className={`flex items-center justify-between px-5 py-3.5 border-b ${
            isDark ? 'border-[#333] bg-[#1e1e1e]' : 'border-neutral-200 bg-neutral-100'
          }`}
        >
          <div className="flex items-center gap-2.5">
            <div className="w-8 h-8 rounded-lg bg-linear-to-br from-[#E95420] to-[#77216F] flex items-center justify-center text-white shadow-sm font-bold text-sm">
              <Download className="w-4 h-4" />
            </div>
            <div>
              <h2 className="text-sm font-semibold tracking-tight flex items-center gap-2">
                Install Ubuntu Notepad
                {isStandalone && (
                  <span className="text-[10px] px-1.5 py-0.5 rounded bg-emerald-500/20 text-emerald-400 font-normal border border-emerald-500/30">
                    Installed & Active
                  </span>
                )}
              </h2>
              <p className="text-[11px] text-neutral-400">
                Choose the fastest, most seamless way to install on your Ubuntu desktop
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className={`w-7 h-7 rounded-full flex items-center justify-center text-sm transition-colors cursor-pointer ${
              isDark
                ? 'hover:bg-neutral-700 text-neutral-400 hover:text-white'
                : 'hover:bg-neutral-200 text-neutral-600 hover:text-neutral-950'
            }`}
            aria-label="Close"
          >
            ✕
          </button>
        </div>

        {/* Method Switcher Tabs */}
        <div
          className={`flex border-b px-5 pt-2 gap-2 text-xs font-medium ${
            isDark ? 'border-[#333] bg-[#222]' : 'border-neutral-200 bg-neutral-50'
          }`}
        >
          <button
            onClick={() => setActiveTab('pwa')}
            className={`pb-2.5 px-3 border-b-2 flex items-center gap-2 cursor-pointer transition-colors ${
              activeTab === 'pwa'
                ? isDark
                  ? 'border-white text-white font-semibold'
                  : 'border-neutral-950 text-neutral-950 font-semibold'
                : 'border-transparent text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <Zap className="w-3.5 h-3.5 text-amber-500" />
            <span>1-Click Desktop App (PWA)</span>
            <span className="text-[10px] px-1.5 py-0.2 rounded bg-neutral-500/20 text-neutral-300">
              Instant
            </span>
          </button>

          <button
            onClick={() => setActiveTab('deb')}
            className={`pb-2.5 px-3 border-b-2 flex items-center gap-2 cursor-pointer transition-colors ${
              activeTab === 'deb'
                ? isDark
                  ? 'border-white text-white font-semibold'
                  : 'border-neutral-950 text-neutral-950 font-semibold'
                : 'border-transparent text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <Box className="w-3.5 h-3.5 text-blue-400" />
            <span>Ubuntu .deb Package</span>
          </button>

          <button
            onClick={() => setActiveTab('script')}
            className={`pb-2.5 px-3 border-b-2 flex items-center gap-2 cursor-pointer transition-colors ${
              activeTab === 'script'
                ? isDark
                  ? 'border-white text-white font-semibold'
                  : 'border-neutral-950 text-neutral-950 font-semibold'
                : 'border-transparent text-neutral-400 hover:text-neutral-200'
            }`}
          >
            <Terminal className="w-3.5 h-3.5 text-emerald-400" />
            <span>1-Command Terminal Setup</span>
          </button>
        </div>

        {/* Content Body */}
        <div className="flex-1 overflow-y-auto p-5 space-y-4 text-xs">
          {/* TAB 1: 1-Click PWA Desktop App */}
          {activeTab === 'pwa' && (
            <div className="space-y-4">
              <div
                className={`p-4 rounded-xl border flex flex-col sm:flex-row items-start sm:items-center justify-between gap-4 ${
                  isDark
                    ? 'bg-neutral-800/80 border-neutral-700'
                    : 'bg-white border-neutral-200 shadow-xs'
                }`}
              >
                <div className="space-y-1">
                  <div className="flex items-center gap-2">
                    <span className="font-semibold text-sm">
                      Seamless Native Ubuntu Integration
                    </span>
                    <span className="text-[10px] px-2 py-0.5 rounded-full bg-emerald-500/20 text-emerald-400 font-medium">
                      Recommended
                    </span>
                  </div>
                  <p className="text-neutral-400 text-[11px] leading-relaxed max-w-md">
                    Installs instantly with zero compiling or terminal commands. Appears in your
                    Ubuntu Applications launcher (<kbd className="px-1 py-0.5 rounded bg-neutral-700 font-mono text-[10px]">Super</kbd> key) and docks right to your Ubuntu side panel with its own native window.
                  </p>
                </div>

                <div className="shrink-0 flex items-center gap-2">
                  {deferredPrompt ? (
                    <button
                      type="button"
                      onClick={onTriggerPwaInstall}
                      className="px-4 py-2 rounded-lg bg-white text-neutral-950 hover:bg-neutral-200 font-semibold text-xs flex items-center gap-2 shadow cursor-pointer transition-all active:scale-95"
                    >
                      <Download className="w-4 h-4 text-neutral-950" />
                      <span>Install Now (1-Click)</span>
                    </button>
                  ) : (
                    <div className="text-right">
                      <span className="text-[11px] font-medium text-neutral-300 block">
                        Browser Install Ready
                      </span>
                      <span className="text-[10px] text-neutral-500">
                        See browser instructions below
                      </span>
                    </div>
                  )}
                </div>
              </div>

              {/* Step by step for Chrome / Chromium / Firefox / Brave */}
              <div
                className={`p-4 rounded-lg border space-y-3 ${
                  isDark ? 'bg-neutral-900/50 border-neutral-800' : 'bg-neutral-100 border-neutral-200'
                }`}
              >
                <div className="font-semibold text-neutral-200 flex items-center gap-2">
                  <Monitor className="w-4 h-4 text-neutral-400" />
                  <span>How to install in 2 clicks on Ubuntu:</span>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-[11px]">
                  <div
                    className={`p-3 rounded border ${
                      isDark ? 'bg-[#202020] border-[#333]' : 'bg-white border-neutral-200'
                    }`}
                  >
                    <div className="font-medium text-neutral-200 mb-1 flex items-center gap-1.5">
                      <span className="w-2 h-2 rounded-full bg-blue-400" />
                      <span>Chrome / Chromium / Brave</span>
                    </div>
                    <ol className="list-decimal list-inside space-y-1 text-neutral-400">
                      <li>Look at the top URL address bar on the right.</li>
                      <li>
                        Click the{' '}
                        <strong className="text-neutral-200">"Install"</strong> icon (or menu ⋮ →{' '}
                        <em>"Save and share"</em> → <em>"Install page as app"</em>).
                      </li>
                      <li>
                        Click <strong className="text-neutral-200">Install</strong>. It launches in its own native window!
                      </li>
                    </ol>
                  </div>

                  <div
                    className={`p-3 rounded border ${
                      isDark ? 'bg-[#202020] border-[#333]' : 'bg-white border-neutral-200'
                    }`}
                  >
                    <div className="font-medium text-neutral-200 mb-1 flex items-center gap-1.5">
                      <span className="w-2 h-2 rounded-full bg-orange-400" />
                      <span>Firefox on Ubuntu</span>
                    </div>
                    <ol className="list-decimal list-inside space-y-1 text-neutral-400">
                      <li>
                        Use the <strong>1-Command Terminal Setup</strong> tab above to create an instant desktop launcher.
                      </li>
                      <li>
                        Or install the native GNOME <strong>Web Apps</strong> extension to run it as a standalone GTK window.
                      </li>
                    </ol>
                  </div>
                </div>

                <div className="flex items-center gap-2 text-[11px] text-neutral-400 pt-1 border-t border-neutral-800">
                  <ShieldCheck className="w-4 h-4 text-emerald-400 shrink-0" />
                  <span>
                    Works fully offline with service worker caching. Your notes stay private on your local machine.
                  </span>
                </div>
              </div>
            </div>
          )}

          {/* TAB 2: Ubuntu .deb Package */}
          {activeTab === 'deb' && (
            <div className="space-y-4">
              <div
                className={`p-3.5 rounded-lg border flex items-center gap-3 ${
                  isDark ? 'bg-neutral-900/60 border-neutral-800' : 'bg-white border-neutral-200'
                }`}
              >
                <Box className="w-6 h-6 text-blue-400 shrink-0" />
                <div>
                  <div className="font-semibold text-neutral-200">
                    Standard Ubuntu Debian Package (.deb)
                  </div>
                  <div className="text-[11px] text-neutral-400">
                    Creates an official <code className="text-neutral-300">.deb</code> file that installs to{' '}
                    <code>/usr/bin/ubuntu-notepad</code> and integrates into Ubuntu App Center.
                  </div>
                </div>
              </div>

              <div className="space-y-2">
                <span className="font-semibold text-neutral-300 block">
                  Quick 1-Liner to build & install the .deb:
                </span>
                <div
                  className={`p-3 rounded-lg border font-mono text-[11px] relative flex items-center justify-between ${
                    isDark ? 'bg-[#181818] border-[#333]' : 'bg-neutral-100 border-neutral-300'
                  }`}
                >
                  <code className="text-neutral-200 overflow-x-auto pr-16 select-all">
                    cargo install cargo-deb && cargo deb --install
                  </code>
                  <button
                    onClick={() =>
                      copyToClipboard(
                        'cargo install cargo-deb && cargo deb --install',
                        'cargo-deb-cmd'
                      )
                    }
                    className={`absolute right-2 px-2.5 py-1 rounded text-xs flex items-center gap-1 border cursor-pointer transition-colors ${
                      copiedId === 'cargo-deb-cmd'
                        ? 'bg-emerald-600/30 text-emerald-300 border-emerald-500/40'
                        : isDark
                        ? 'bg-neutral-800 hover:bg-neutral-700 text-neutral-300 border-neutral-700'
                        : 'bg-white hover:bg-neutral-100 text-neutral-800 border-neutral-300'
                    }`}
                  >
                    {copiedId === 'cargo-deb-cmd' ? (
                      <>
                        <Check className="w-3 h-3 text-emerald-400" />
                        <span>Copied</span>
                      </>
                    ) : (
                      <>
                        <Copy className="w-3 h-3" />
                        <span>Copy</span>
                      </>
                    )}
                  </button>
                </div>
              </div>

              <div
                className={`p-3.5 rounded-lg border text-[11px] space-y-2 ${
                  isDark ? 'bg-neutral-900/40 border-neutral-800' : 'bg-neutral-50 border-neutral-200'
                }`}
              >
                <div className="font-semibold text-neutral-300">
                  What does the .deb installer do?
                </div>
                <ul className="list-disc list-inside space-y-1 text-neutral-400">
                  <li>
                    Installs the compiled binary to{' '}
                    <code className="text-neutral-300">/usr/bin/ubuntu-notepad</code>
                  </li>
                  <li>
                    Puts the launcher in{' '}
                    <code className="text-neutral-300">
                      /usr/share/applications/ubuntu-notepad.desktop
                    </code>
                  </li>
                  <li>
                    Adds the icon to system theme so it appears in Ubuntu Dock & Dash
                  </li>
                  <li>
                    Can be removed anytime cleanly with{' '}
                    <code className="text-neutral-300">sudo apt remove ubuntu-notepad</code>
                  </li>
                </ul>
              </div>
            </div>
          )}

          {/* TAB 3: 1-Command Terminal Setup */}
          {activeTab === 'script' && (
            <div className="space-y-4">
              <div
                className={`p-3.5 rounded-lg border flex items-center justify-between gap-3 ${
                  isDark ? 'bg-neutral-900/60 border-neutral-800' : 'bg-white border-neutral-200'
                }`}
              >
                <div>
                  <div className="font-semibold text-neutral-200">
                    Instant Ubuntu Desktop Launcher Script
                  </div>
                  <div className="text-[11px] text-neutral-400">
                    Creates an authentic Ubuntu Dock icon and menu shortcut in{' '}
                    <code>~/.local/share/applications/</code> without needing root password.
                  </div>
                </div>

                <button
                  type="button"
                  onClick={downloadScriptFile}
                  className="px-3 py-1.5 rounded-md bg-white text-neutral-950 hover:bg-neutral-200 font-semibold text-xs flex items-center gap-1.5 shrink-0 cursor-pointer shadow-xs transition-colors"
                >
                  <FolderDown className="w-3.5 h-3.5" />
                  <span>Download .sh</span>
                </button>
              </div>

              <div className="space-y-2">
                <span className="font-semibold text-neutral-300 block">
                  Run directly in Ubuntu Terminal (paste & enter):
                </span>
                <div
                  className={`p-3 rounded-lg border font-mono text-[11px] relative flex items-center justify-between ${
                    isDark ? 'bg-[#181818] border-[#333]' : 'bg-neutral-100 border-neutral-300'
                  }`}
                >
                  <code className="text-neutral-200 overflow-x-auto pr-16 select-all">
                    curl -sSL {typeof window !== 'undefined' ? window.location.origin : ''}/install-ubuntu-notepad.sh | bash
                  </code>
                  <button
                    onClick={() =>
                      copyToClipboard(
                        `curl -sSL ${typeof window !== 'undefined' ? window.location.origin : ''}/install-ubuntu-notepad.sh | bash`,
                        'curl-script-cmd'
                      )
                    }
                    className={`absolute right-2 px-2.5 py-1 rounded text-xs flex items-center gap-1 border cursor-pointer transition-colors ${
                      copiedId === 'curl-script-cmd'
                        ? 'bg-emerald-600/30 text-emerald-300 border-emerald-500/40'
                        : isDark
                        ? 'bg-neutral-800 hover:bg-neutral-700 text-neutral-300 border-neutral-700'
                        : 'bg-white hover:bg-neutral-100 text-neutral-800 border-neutral-300'
                    }`}
                  >
                    {copiedId === 'curl-script-cmd' ? (
                      <>
                        <Check className="w-3 h-3 text-emerald-400" />
                        <span>Copied</span>
                      </>
                    ) : (
                      <>
                        <Copy className="w-3 h-3" />
                        <span>Copy</span>
                      </>
                    )}
                  </button>
                </div>
              </div>

              {/* Script code preview */}
              <div className="space-y-1.5">
                <span className="text-neutral-400 text-[11px] block">
                  Script Source Code (install-ubuntu-notepad.sh):
                </span>
                <pre
                  className={`p-3 rounded-lg font-mono text-[11px] border overflow-x-auto max-h-40 ${
                    isDark
                      ? 'bg-[#151515] border-[#333] text-neutral-300'
                      : 'bg-white border-neutral-200 text-neutral-800'
                  }`}
                >
                  {oneClickScript}
                </pre>
              </div>
            </div>
          )}
        </div>

        {/* Modal Footer */}
        <div
          className={`flex items-center justify-between px-5 py-3 border-t text-xs ${
            isDark ? 'border-[#333] bg-[#1e1e1e]' : 'border-neutral-200 bg-neutral-100'
          }`}
        >
          <div className="text-neutral-400 text-[11px] flex items-center gap-1.5">
            <span className="w-2 h-2 rounded-full bg-emerald-400" />
            <span>Ubuntu 22.04 LTS & 24.04 LTS Compatible</span>
          </div>

          <button
            onClick={onClose}
            className={`px-4 py-1.5 rounded-md font-medium cursor-pointer transition-colors ${
              isDark
                ? 'bg-neutral-700 hover:bg-neutral-600 text-neutral-200'
                : 'bg-neutral-200 hover:bg-neutral-300 text-neutral-800'
            }`}
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
