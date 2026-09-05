# Phase 0 — toolchain

What has to exist on the machine before `npm run tauri:dev` will open a window.
Windows is the primary target; macOS notes follow.

## Windows 10 / 11

### 1. Microsoft C++ Build Tools

Rust on Windows links with MSVC, so this is required even though you will not
open Visual Studio.

Install **Build Tools for Visual Studio** and select the
_Desktop development with C++_ workload. That gives you the MSVC linker and the
Windows SDK. This is the step people skip, and the failure it produces is a
`link.exe not found` error much later.

### 2. WebView2

Tauri renders the UI in the system webview. On Windows 11 and up-to-date
Windows 10 this is already present. Otherwise install the **WebView2 Evergreen
Bootstrapper** from Microsoft.

This is the reason Tauri binaries are small: there is no bundled Chromium.
It is also the reason the app must be tested on Windows — the webview there is
Chromium-based, while on macOS it is WebKit, and they differ.

### 3. Rust

Install via `rustup` from https://rustup.rs. Take the default
`stable-x86_64-pc-windows-msvc` toolchain.

```powershell
rustc --version    # 1.85 or newer — the crate uses edition 2024
cargo --version
```

Two components worth adding now, both used throughout the project:

```powershell
rustup component add clippy rustfmt
```

`clippy` is the linter, and for someone learning Rust it is genuinely
instructive — it explains idiom, not just style. `rustfmt` ends formatting
arguments.

### 4. Node

Node 20 or newer, from https://nodejs.org (LTS). `npm` ships with it.

```powershell
node -v
npm -v
```

### 5. Git

From https://git-scm.com. Set your identity before the first commit:

```powershell
git config --global user.name "Your Name"
git config --global user.email "you@example.com"
```

### 6. Verify

```powershell
git clone https://github.com/vermasaksham/Sutra-Windows.git
cd Sutra-Windows
npm install
npm run tauri:dev
```

The first `tauri:dev` compiles the whole Rust dependency tree and takes several
minutes. Later runs are seconds, because cargo caches everything in
`src-tauri/target/`. Frontend edits hot-reload; Rust edits trigger a rebuild
and restart the window.

## macOS

Kept working where it is free, but not the primary target.

```bash
xcode-select --install                     # Command Line Tools
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
brew install node
```

WebKit is part of the OS, so there is no WebView2 equivalent to install.

## Linux (CI and container builds only)

Not a supported target for the app, but useful if you ever build in CI. On
Debian/Ubuntu:

```bash
sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev \
  build-essential curl wget file libssl-dev pkg-config
```

## Troubleshooting

**`link.exe not found`** — the C++ Build Tools workload from step 1 is missing
or the _Desktop development with C++_ workload was not ticked.

**The window opens blank** — the Vite dev server is not on port 1420.
`vite.config.ts` sets `strictPort: true` on purpose: if something else holds
1420, Vite fails loudly instead of quietly moving to 1421 and leaving Tauri
pointed at nothing.

**First build seems hung** — it is not. Compiling Tauri from scratch is a few
hundred crates. Watch `src-tauri/target/` grow.

**`npm run dev` shows "not running inside Tauri"** — expected. That script
serves the frontend in a plain browser, where there is no Rust host to answer
`invoke`. Use `npm run tauri:dev` for the real app.
