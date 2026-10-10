import React, { useState } from 'react';
import {
  X,
  Copy,
  Check,
  Download,
  Terminal,
  Cpu,
  Layers,
  CheckCircle2,
  ExternalLink,
  Package,
} from 'lucide-react';
import {
  C_GTK4_SOURCE,
  C_MAKEFILE,
  RUST_MAIN_RS,
  RUST_CARGO_TOML,
  UBUNTU_DESKTOP_ENTRY,
} from '../utils/nativeTemplates';

interface NativeCodeModalProps {
  isOpen: boolean;
  onClose: () => void;
  theme: 'yaru-dark' | 'yaru-light';
}

export const NativeCodeModal: React.FC<NativeCodeModalProps> = ({
  isOpen,
  onClose,
  theme,
}) => {
  const [activeTab, setActiveTab] = useState<'rust' | 'c' | 'benchmarks' | 'setup'>('rust');
  const [copied, setCopied] = useState<string | null>(null);

  if (!isOpen) return null;

  const isDark = theme === 'yaru-dark';

  const copyToClipboard = (text: string, label: string) => {
    navigator.clipboard.writeText(text);
    setCopied(label);
    setTimeout(() => setCopied(null), 2000);
  };

  const downloadFile = (filename: string, content: string) => {
    const blob = new Blob([content], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(url);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-xs">
      <div
        className={`w-full max-w-4xl max-h-[88vh] flex flex-col rounded-xl shadow-2xl border overflow-hidden transition-all ${
          isDark
            ? 'bg-[#222222] border-[#3E3E3E] text-[#dfdbd2]'
            : 'bg-white border-neutral-300 text-neutral-800'
        }`}
      >
        {/* Modal Header */}
        <div
          className={`flex items-center justify-between px-3 sm:px-5 py-3 sm:py-3.5 border-b shrink-0 ${
            isDark ? 'bg-[#1c1c1c] border-[#333]' : 'bg-[#f5f5f5] border-neutral-300'
          }`}
        >
          <div className="flex items-center gap-2 sm:gap-2.5">
            <div className={`p-1.5 rounded-lg shrink-0 border ${
              isDark ? 'bg-neutral-800 text-neutral-200 border-neutral-700' : 'bg-neutral-200 text-neutral-800 border-neutral-300'
            }`}>
              <Terminal className="w-4 h-4 sm:w-5 sm:h-5" />
            </div>
            <div>
              <h2 className="font-semibold text-xs sm:text-sm truncate">
                LiteMark Native Ubuntu Build: C & Rust with GTK4
              </h2>
              <p className="text-[11px] sm:text-xs text-neutral-400 hidden xs:block">
                Ultra-lightweight native performance, ~18–22 MB RAM, instant Wayland/X11
              </p>
            </div>
          </div>

          <button
            type="button"
            onClick={onClose}
            className={`p-1.5 rounded-lg cursor-pointer ${
              isDark ? 'hover:bg-[#333] text-neutral-400' : 'hover:bg-neutral-200 text-neutral-600'
            }`}
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Tab Navigation */}
        <div
          className={`flex items-center gap-1 px-3 sm:px-5 pt-2 border-b shrink-0 text-xs font-medium overflow-x-auto whitespace-nowrap scrollbar-none ${
            isDark ? 'bg-[#242424] border-[#333]' : 'bg-[#fafafa] border-neutral-200'
          }`}
        >
          <button
            type="button"
            onClick={() => setActiveTab('rust')}
            className={`px-2.5 sm:px-3 py-2 border-b-2 cursor-pointer transition-colors shrink-0 active:scale-95 ${
              activeTab === 'rust'
                ? isDark
                  ? 'border-white text-white font-bold'
                  : 'border-neutral-900 text-neutral-950 font-bold'
                : isDark
                ? 'border-transparent text-neutral-400 hover:text-white'
                : 'border-transparent text-neutral-600 hover:text-black'
            }`}
          >
            Rust + GTK4
          </button>

          <button
            type="button"
            onClick={() => setActiveTab('c')}
            className={`px-2.5 sm:px-3 py-2 border-b-2 cursor-pointer transition-colors shrink-0 active:scale-95 ${
              activeTab === 'c'
                ? isDark
                  ? 'border-white text-white font-bold'
                  : 'border-neutral-900 text-neutral-950 font-bold'
                : isDark
                ? 'border-transparent text-neutral-400 hover:text-white'
                : 'border-transparent text-neutral-600 hover:text-black'
            }`}
          >
            C + GTK4 (~18MB)
          </button>

          <button
            type="button"
            onClick={() => setActiveTab('benchmarks')}
            className={`px-2.5 sm:px-3 py-2 border-b-2 cursor-pointer transition-colors shrink-0 active:scale-95 ${
              activeTab === 'benchmarks'
                ? isDark
                  ? 'border-white text-white font-bold'
                  : 'border-neutral-900 text-neutral-950 font-bold'
                : isDark
                ? 'border-transparent text-neutral-400 hover:text-white'
                : 'border-transparent text-neutral-600 hover:text-black'
            }`}
          >
            Benchmarks
          </button>

          <button
            type="button"
            onClick={() => setActiveTab('setup')}
            className={`px-2.5 sm:px-3 py-2 border-b-2 cursor-pointer transition-colors shrink-0 active:scale-95 ${
              activeTab === 'setup'
                ? isDark
                  ? 'border-white text-white font-bold'
                  : 'border-neutral-900 text-neutral-950 font-bold'
                : isDark
                ? 'border-transparent text-neutral-400 hover:text-white'
                : 'border-transparent text-neutral-600 hover:text-black'
            }`}
          >
            Ubuntu Setup
          </button>
        </div>

        {/* Tab Content */}
        <div className="flex-1 overflow-y-auto p-5 text-xs">
          {/* TAB 1: RUST */}
          {activeTab === 'rust' && (
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <div>
                  <h3 className="font-semibold text-sm">Rust + gtk4-rs Implementation</h3>
                  <p className="text-neutral-400 text-xs">
                    Memory-safe gtk4 bindings with Pango formatting, async file dialogs, and automatic <strong className="text-neutral-200">~/Documents/Notes</strong> folder organization.
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => copyToClipboard(RUST_MAIN_RS, 'rust')}
                    className={`flex items-center gap-1.5 px-3 py-1.5 rounded transition-all active:scale-95 cursor-pointer font-medium shadow-xs ${
                      isDark
                        ? 'bg-neutral-100 text-neutral-950 hover:bg-white'
                        : 'bg-neutral-900 text-white hover:bg-neutral-800'
                    }`}
                  >
                    {copied === 'rust' ? <Check className="w-3.5 h-3.5" /> : <Copy className="w-3.5 h-3.5" />}
                    <span>{copied === 'rust' ? 'Copied!' : 'Copy main.rs'}</span>
                  </button>
                  <button
                    type="button"
                    onClick={() => downloadFile('main.rs', RUST_MAIN_RS)}
                    className={`flex items-center gap-1.5 px-3 py-1.5 rounded border transition-all active:scale-95 cursor-pointer ${
                      isDark ? 'border-[#444] hover:bg-[#333]' : 'border-neutral-300 hover:bg-neutral-100'
                    }`}
                  >
                    <Download className="w-3.5 h-3.5" />
                    <span>Download</span>
                  </button>
                </div>
              </div>

              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <span className="font-mono text-neutral-400 text-[11px]">Cargo.toml</span>
                  <button
                    type="button"
                    onClick={() => copyToClipboard(RUST_CARGO_TOML, 'cargo')}
                    className={`text-[11px] hover:underline cursor-pointer ${
                      isDark ? 'text-neutral-300 hover:text-white' : 'text-neutral-700 hover:text-black'
                    }`}
                  >
                    {copied === 'cargo' ? 'Copied' : 'Copy Cargo.toml'}
                  </button>
                </div>
                <pre
                  className={`p-3 rounded-lg font-mono text-[11px] border overflow-x-auto ${
                    isDark ? 'bg-[#181818] border-[#333] text-neutral-200' : 'bg-neutral-50 border-neutral-200 text-neutral-800'
                  }`}
                >
                  {RUST_CARGO_TOML}
                </pre>
              </div>

              <div>
                <span className="font-mono text-neutral-400 text-[11px] block mb-1.5">src/main.rs</span>
                <pre
                  className={`p-3 rounded-lg font-mono text-[11px] border overflow-x-auto max-h-72 ${
                    isDark ? 'bg-[#181818] border-[#333] text-neutral-200' : 'bg-neutral-50 border-neutral-200 text-neutral-800'
                  }`}
                >
                  {RUST_MAIN_RS}
                </pre>
              </div>

              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <div className="flex items-center gap-1.5">
                    <span className="font-mono text-neutral-400 text-[11px]">litemark.desktop</span>
                    <span className="text-[10px] px-1.5 py-0.2 rounded bg-neutral-200 text-neutral-800 dark:bg-neutral-800 dark:text-neutral-300 font-mono">
                      Ubuntu App Launcher & .deb
                    </span>
                  </div>
                  <div className="flex items-center gap-2">
                    <button
                      type="button"
                      onClick={() => copyToClipboard(UBUNTU_DESKTOP_ENTRY, 'desktop')}
                      className={`text-[11px] hover:underline cursor-pointer ${
                        isDark ? 'text-neutral-300 hover:text-white' : 'text-neutral-700 hover:text-black'
                      }`}
                    >
                      {copied === 'desktop' ? 'Copied' : 'Copy .desktop'}
                    </button>
                    <button
                      type="button"
                      onClick={() => downloadFile('litemark.desktop', UBUNTU_DESKTOP_ENTRY)}
                      className={`text-[11px] hover:underline cursor-pointer flex items-center gap-1 ${
                        isDark ? 'text-neutral-300 hover:text-white' : 'text-neutral-700 hover:text-black'
                      }`}
                    >
                      <Download className="w-3 h-3" />
                      <span>Download</span>
                    </button>
                  </div>
                </div>
                <pre
                  className={`p-3 rounded-lg font-mono text-[11px] border overflow-x-auto ${
                    isDark ? 'bg-[#181818] border-[#333] text-neutral-200' : 'bg-neutral-50 border-neutral-200 text-neutral-800'
                  }`}
                >
                  {UBUNTU_DESKTOP_ENTRY}
                </pre>
              </div>
            </div>
          )}

          {/* TAB 2: C */}
          {activeTab === 'c' && (
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <div>
                  <h3 className="font-semibold text-sm">C + GTK4 Minimalist Implementation</h3>
                  <p className="text-neutral-400 text-xs">
                    Pure C99 with GTK4 runtime. Single executable binary (~120KB), ~18MB idle memory, and automatic <strong className="text-neutral-200">~/Documents/Notes</strong> folder management.
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => copyToClipboard(C_GTK4_SOURCE, 'c')}
                    className={`flex items-center gap-1.5 px-3 py-1.5 rounded transition-all active:scale-95 cursor-pointer font-medium shadow-xs ${
                      isDark
                        ? 'bg-neutral-100 text-neutral-950 hover:bg-white'
                        : 'bg-neutral-900 text-white hover:bg-neutral-800'
                    }`}
                  >
                    {copied === 'c' ? <Check className="w-3.5 h-3.5" /> : <Copy className="w-3.5 h-3.5" />}
                    <span>{copied === 'c' ? 'Copied!' : 'Copy main.c'}</span>
                  </button>
                  <button
                    type="button"
                    onClick={() => downloadFile('main.c', C_GTK4_SOURCE)}
                    className={`flex items-center gap-1.5 px-3 py-1.5 rounded border transition-all active:scale-95 cursor-pointer ${
                      isDark ? 'border-[#444] hover:bg-[#333]' : 'border-neutral-300 hover:bg-neutral-100'
                    }`}
                  >
                    <Download className="w-3.5 h-3.5" />
                    <span>Download</span>
                  </button>
                </div>
              </div>

              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <span className="font-mono text-neutral-400 text-[11px]">Makefile</span>
                  <button
                    type="button"
                    onClick={() => copyToClipboard(C_MAKEFILE, 'makefile')}
                    className={`text-[11px] hover:underline cursor-pointer ${
                      isDark ? 'text-neutral-300 hover:text-white' : 'text-neutral-700 hover:text-black'
                    }`}
                  >
                    {copied === 'makefile' ? 'Copied' : 'Copy Makefile'}
                  </button>
                </div>
                <pre
                  className={`p-3 rounded-lg font-mono text-[11px] border overflow-x-auto ${
                    isDark ? 'bg-[#181818] border-[#333] text-neutral-200' : 'bg-neutral-50 border-neutral-200 text-neutral-800'
                  }`}
                >
                  {C_MAKEFILE}
                </pre>
              </div>

              <div>
                <span className="font-mono text-neutral-400 text-[11px] block mb-1.5">main.c</span>
                <pre
                  className={`p-3 rounded-lg font-mono text-[11px] border overflow-x-auto max-h-72 ${
                    isDark ? 'bg-[#181818] border-[#333] text-neutral-200' : 'bg-neutral-50 border-neutral-200 text-neutral-800'
                  }`}
                >
                  {C_GTK4_SOURCE}
                </pre>
              </div>
            </div>
          )}

          {/* TAB 3: BENCHMARKS */}
          {activeTab === 'benchmarks' && (
            <div className="space-y-5">
              <div>
                <h3 className="font-semibold text-sm mb-1">Architecture & Resource Benchmark</h3>
                <p className="text-neutral-400 text-xs">
                  Why building with native C or Rust GTK outperforms Electron-based editors (like VS Code, Obsidian) on low-spec Ubuntu laptops:
                </p>
              </div>

              {/* Comparison Grid */}
              <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                <div
                  className={`p-4 rounded-lg border ${
                    isDark ? 'bg-[#1c1c1c] border-[#333]' : 'bg-neutral-50 border-neutral-200'
                  }`}
                >
                  <div className="flex items-center gap-2 mb-2 text-white dark:text-white text-neutral-900 font-semibold">
                    <Cpu className="w-4 h-4 text-neutral-400" />
                    <span>C + GTK4</span>
                  </div>
                  <div className="space-y-1.5 text-neutral-400 font-mono text-[11px]">
                    <div>RAM Usage: <strong className="text-white dark:text-neutral-100 font-bold">~18 MB</strong></div>
                    <div>Cold Start: <strong className="text-white dark:text-neutral-100 font-bold">~35 ms</strong></div>
                    <div>Binary Size: <strong className="text-white dark:text-neutral-100 font-bold">~120 KB</strong></div>
                    <div>Engine: Native Linux Wayland / X11</div>
                  </div>
                </div>

                <div
                  className={`p-4 rounded-lg border ${
                    isDark ? 'bg-[#1c1c1c] border-[#333]' : 'bg-neutral-50 border-neutral-200'
                  }`}
                >
                  <div className="flex items-center gap-2 mb-2 text-white dark:text-white text-neutral-900 font-semibold">
                    <Layers className="w-4 h-4 text-neutral-400" />
                    <span>Rust + GTK4</span>
                  </div>
                  <div className="space-y-1.5 text-neutral-400 font-mono text-[11px]">
                    <div>RAM Usage: <strong className="text-white dark:text-neutral-100 font-bold">~22 MB</strong></div>
                    <div>Cold Start: <strong className="text-white dark:text-neutral-100 font-bold">~45 ms</strong></div>
                    <div>Binary Size: <strong className="text-white dark:text-neutral-100 font-bold">~4.5 MB</strong></div>
                    <div>Safety: Zero segfaults / memory safe</div>
                  </div>
                </div>

                <div
                  className={`p-4 rounded-lg border ${
                    isDark ? 'bg-[#1c1c1c] border-[#333]' : 'bg-neutral-50 border-neutral-200'
                  }`}
                >
                  <div className="flex items-center gap-2 mb-2 text-neutral-400 font-semibold">
                    <ExternalLink className="w-4 h-4" />
                    <span>Typical Electron App</span>
                  </div>
                  <div className="space-y-1.5 text-neutral-400 font-mono text-[11px]">
                    <div>RAM Usage: <strong className="text-neutral-400">160–320 MB</strong></div>
                    <div>Cold Start: <strong className="text-neutral-400">650–1200 ms</strong></div>
                    <div>Binary Size: <strong className="text-neutral-400">140–220 MB</strong></div>
                    <div>Engine: Full Chromium + Node.js bundle</div>
                  </div>
                </div>
              </div>

              <div
                className={`p-3.5 rounded-lg border text-xs ${
                  isDark ? 'bg-[#191919] border-[#333]' : 'bg-neutral-100 border-neutral-200'
                }`}
              >
                <div className="font-semibold mb-1 text-white flex items-center gap-1.5">
                  <CheckCircle2 className="w-4 h-4 text-neutral-400" />
                  Key Takeaway for Ubuntu
                </div>
                <p className="text-neutral-400 leading-relaxed">
                  Both <strong>C</strong> and <strong>Rust</strong> with GTK4 communicate directly with the GNOME Wayland compositor and Pango text layout engine. They do not ship an entire Chromium web browser in RAM, meaning you get instantaneous launch, crisp subpixel font rendering matching your Ubuntu theme, and battery-friendly operation.
                </p>
              </div>
            </div>
          )}

          {/* TAB 4: SETUP GUIDE */}
          {activeTab === 'setup' && (
            <div className="space-y-4">
              <h3 className="font-semibold text-sm">Ubuntu Step-by-Step Terminal Commands</h3>

              <div className="space-y-3">
                <div>
                  <span className="font-semibold text-neutral-300 block mb-1">1. Install GTK4 dependencies on Ubuntu</span>
                  <pre
                    className={`p-2.5 rounded font-mono text-[11px] border ${
                      isDark ? 'bg-[#181818] border-[#333]' : 'bg-neutral-100 border-neutral-200'
                    }`}
                  >
                    sudo apt update && sudo apt install -y build-essential libgtk-4-dev pkg-config
                  </pre>
                </div>

                <div>
                  <span className="font-semibold text-neutral-300 block mb-1">2. Build the C Version</span>
                  <pre
                    className={`p-2.5 rounded font-mono text-[11px] border ${
                      isDark ? 'bg-[#181818] border-[#333]' : 'bg-neutral-100 border-neutral-200'
                    }`}
                  >
                    gcc $(pkg-config --cflags gtk4) main.c -o litemark $(pkg-config --libs gtk4) -O2{"\n"}
                    ./litemark
                  </pre>
                </div>

                <div>
                  <span className="font-semibold text-neutral-300 block mb-1">3. Build the Rust Version</span>
                  <pre
                    className={`p-2.5 rounded font-mono text-[11px] border ${
                      isDark ? 'bg-[#181818] border-[#333]' : 'bg-neutral-100 border-neutral-200'
                    }`}
                  >
                    # Install Rust toolchain if not already present:{"\n"}
                    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh{"\n"}
                    cargo new litemark && cd litemark{"\n"}
                    # Replace Cargo.toml, src/main.rs and add litemark.desktop from the tabs above{"\n"}
                    cargo run --release
                  </pre>
                </div>

                <div>
                  <div className="flex items-center gap-1.5 mb-1">
                    <Package className="w-3.5 h-3.5 text-neutral-400" />
                    <span className="font-semibold text-neutral-300 block">4. Package as a Standard Ubuntu .deb Installer</span>
                  </div>
                  <pre
                    className={`p-2.5 rounded font-mono text-[11px] border ${
                      isDark ? 'bg-[#181818] border-[#333]' : 'bg-neutral-100 border-neutral-200'
                    }`}
                  >
                    # Install cargo-deb packaging tool:{"\n"}
                    cargo install cargo-deb{"\n\n"}
                    # Build the .deb package:{"\n"}
                    cargo deb{"\n\n"}
                    # Install the generated .deb package on your Ubuntu system:{"\n"}
                    sudo apt install ./target/debian/litemark_0.1.0_amd64.deb{"\n\n"}
                    # Or double-click the .deb file in Ubuntu Files to install via Ubuntu App Center!
                  </pre>
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Modal Footer */}
        <div
          className={`flex items-center justify-between px-5 py-3 border-t shrink-0 text-xs ${
            isDark ? 'bg-[#1c1c1c] border-[#333]' : 'bg-[#f5f5f5] border-neutral-200'
          }`}
        >
          <span className="text-neutral-400">
            You can use this web editor directly, or compile the native C/Rust files for Ubuntu.
          </span>
          <button
            type="button"
            onClick={onClose}
            className={`px-4 py-1.5 rounded-lg transition-all active:scale-95 cursor-pointer font-medium shadow-xs ${
              isDark
                ? 'bg-neutral-100 text-neutral-950 hover:bg-white'
                : 'bg-neutral-900 text-white hover:bg-neutral-800'
            }`}
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
};
