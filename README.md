<img width="1664" height="928" alt="1787690128" src="https://github.com/user-attachments/assets/ad7ef8eb-9709-4ffa-8f1e-9da1f39de0f3" />

# Surf FED — Tauri Edition (v2.0.0)

A native rewrite of the **Surf FED** web browser, migrated from Electron to
**Tauri v2**.  Instead of bundling a full Chromium runtime, Surf FED now uses
the operating system's native WebView (WKWebView on macOS, WebView2 on
Windows, WebKitGTK on Linux) driven by a Rust backend.  The result is a
dramatically smaller binary, lower memory footprint, and significantly better
battery life on Apple devices — the exact benefits that motivated this
migration.

---

## Why Tauri? (Electron → Tauri)

| Metric                     | Electron (v1.x)           | Tauri v2 (this project)         |
| -------------------------- | ------------------------- | ------------------------------- |
| Bundled browser engine     | Full Chromium (~150 MB)   | System WebView (0 MB bundled)   |
| Backend language           | JavaScript (Node.js)      | Rust (compiled, no runtime)     |
| Typical app binary         | ~170 MB                   | ~8–15 MB                        |
| RAM usage (one window)     | ~250–400 MB               | ~80–150 MB                      |
| macOS battery impact       | High (Chromium process tree) | Low (native WKWebView)       |
| Startup time               | ~2–4 s                    | ~0.3–0.8 s                      |

On Apple devices specifically, WKWebView is the same engine Safari uses, which
means Surf FED benefits from Apple's hardware-accelerated rendering pipeline,
Metal-backed compositing, and the aggressive power management built into macOS
and iOS.  There is no helper-process sprawl — Tauri uses a single native
process with the OS-provided webview, so there is no Chromium GPU process,
utility process, or renderer process tree consuming memory and energy.

---

## Architecture Overview

```
surf-fed-tauri/
├── package.json              # Frontend build scripts (Vite)
├── vite.config.js            # Vite config (vanilla JS, no framework)
├── src/                      # Frontend (rendered in the WebView)
│   ├── index.html            # Frameless title bar, toolbar, tabs, panels
│   ├── styles.css            # Full UI styling + dark mode
│   └── renderer.js           # Tab mgmt, navigation, extension UI, popups
│
└── src-tauri/                # Rust backend
    ├── Cargo.toml            # Tauri v2 + reqwest + scraper + tokio deps
    ├── tauri.conf.json       # App identity, frameless window, CSP, bundle
    ├── build.rs              # Tauri build script
    ├── capabilities/
    │   └── default.json      # Tauri v2 opt-in permissions (security model)
    ├── icons/                # App icons
    ├── extensions/builtin/   # Bundled built-in extension assets (rules.json, etc.)
    └── src/
        ├── main.rs           # Entry point (windows_subsystem attribute)
        ├── lib.rs            # App builder, plugin registration, command handler list
        ├── commands.rs       # 19 #[tauri::command] functions exposed to frontend
        ├── extensions.rs     # Extension registry, discovery, state persistence
        ├── blocking.rs       # Native ad-blocker (Rust rule engine)
        └── helpers.rs        # File/path utilities (user dirs, manifest reading)
```

### Key Migration Decisions

**`<webview>` → `<iframe>`**:  Electron's `<webview>` tag (a separate Chromium
renderer with its own JavaScript context) has no direct Tauri equivalent.
Tauri's native WebView renders the app shell itself; external pages are loaded
in `<iframe>` elements with appropriate `sandbox` attributes.  A fallback UI is
shown for sites that block framing via `X-Frame-Options` or CSP
`frame-ancestors`, giving the user an "Open in new window" option.

**`ipcMain.handle` / `contextBridge` → `#[tauri::command]` + `invoke()`**:  All
Electron IPC channels were reimplemented as Tauri commands.  The frontend
calls `await invoke('command_name', { args })` instead of
`ipcRenderer.send()` / `window.api.*`.

**Chrome extensions → native re-implementation**:  Tauri's native WebViews do
not include a Chromium extension runtime, so the four built-in extensions were
re-implemented natively in Rust rather than loaded as Chrome extensions.  The
original extension assets (rules.json, manifests) are bundled as Tauri
resources and consumed by the Rust backend.

**Security model**:  Tauri v2 uses an opt-in capability/permission system.  The
`capabilities/default.json` file grants only the specific permissions the app
needs: window controls, dialog open, shell open, and scoped filesystem access.

---

## The Four Built-in Extensions (Native)

### 1. Ad-Blocker (`blocking.rs`)
Replaces Chromium's `declarativeNetRequest`.  A Rust rule engine loads the
original `rules.json` (same declarativeNetRequest format) at startup and
pre-extracts blocked host substrings for O(n) matching.  The frontend queries
`adblock_check(url)` before navigating to any URL.  If the URL matches a
blocked domain, navigation is cancelled and a "blocked" placeholder is shown.
A default blocklist of 17 major ad/analytics domains is used as a fallback if
`rules.json` is unavailable.

### 2. Dark-Reader (`commands.rs`)
The original extension's CSS filter is stored as a Rust constant
(`DARK_READER_CSS`) — a single source of truth.  The frontend calls
`darkreader_get_css()` to retrieve it and injects it into the active iframe's
`contentDocument` via a `<style>` element.  The toggle state is persisted
across restarts.

### 3. Page-Info (`commands.rs`)
Reads the active iframe's `contentDocument` in the frontend (title, URL,
meta description, link count, image count) and packages it into a `PageInfo`
struct.  The popup displays this information, mirroring the original
extension's popup.

### 4. FED-GRAM — Instagram Image Downloader (`commands.rs`)
The most significant re-implementation.  The original Electron extension
fetched Instagram pages client-side, which was constrained by CORS.  In the
Tauri version, the HTML fetch is performed by Rust using `reqwest` (in a
`tokio::spawn_blocking` task to avoid blocking the async runtime) and parsed
with `scraper` to extract image URLs from `og:image` meta tags and CDN `<img>`
tags.  This completely bypasses CORS.  Downloading uses a native file-save
dialog (`tauri-plugin-dialog`) and streams the image bytes to disk via
`reqwest`.

---

## Prerequisites

### All platforms
- **Rust** 1.77+ (install via [rustup](https://rustup.rs/))
- **Node.js** 20+ and **npm** 10+
- A C/C++ toolchain (gcc/clang/MSVC)

### Linux additional system packages
WebKitGTK development libraries are required for the native WebView:

```bash
# Debian / Ubuntu
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev libgtk-3-dev libglib2.0-dev \
  libjavascriptcoregtk-4.1-dev libsoup-3.0-dev \
  libcairo2-dev libpango1.0-dev libatk1.0-dev \
  libgdk-pixbuf2.0-dev build-essential pkg-config

# Fedora
sudo dnf install -y webkit2gtk4.1-devel gtk3-devel glib2-devel \
  javascriptcoregtk4.1-devel libsoup3-devel cairo-devel pango-devel \
  atk-devel gdk-pixbuf2-devel

# Arch
sudo pacman -S webkit2gtk-4.1 gtk3 glib2 cairo pango atk
```

### macOS
No extra packages needed — Xcode Command Line Tools provide everything
required for WKWebView.

### Windows
No extra packages needed — WebView2 is pre-installed on Windows 10/11.  The
MSVC build tools (via Visual Studio Build Tools) are required for the Rust
compiler.

---

## Building & Running

### Development mode (hot reload)
```bash
cd surf-fed-tauri
npm install
npm run tauri dev
```
This starts the Vite dev server on port 1420 and launches the Tauri app with
live frontend reloading.  Rust changes trigger an automatic rebuild.

### Production build (creates a distributable bundle)
```bash
cd surf-fed-tauri
npm install
npm run tauri build
```
This produces:
- **macOS**: `.app` bundle and `.dmg` installer in `src-tauri/target/release/bundle/`
- **Windows**: `.exe` installer (NSIS or MSI) in `src-tauri/target/release/bundle/`
- **Linux**: `.deb` / `.rpm` / `.AppImage` in `src-tauri/target/release/bundle/`

### Frontend-only build (no native packaging)
```bash
npm run build      # Vite → dist/
npm run preview    # Preview the built frontend in a browser
```

### Rust-only check (fast type-checking without linking)
```bash
cd src-tauri
cargo check        # Type-check without producing a binary
cargo build        # Compile the debug binary
cargo build --release  # Optimized binary
```

---

## Tauri Commands (IPC API)

The frontend communicates with the Rust backend via these `invoke()` commands:

| Command                  | Purpose                                              |
| ------------------------ | --------------------------------------------------- |
| `window_minimize`        | Minimize the frameless window                        |
| `window_maximize`        | Maximize / restore the window                        |
| `window_close`           | Close the app                                        |
| `window_start_dragging`  | Begin dragging the custom title bar                  |
| `extensions_list`        | List all built-in and user extensions with state     |
| `extensions_enable`      | Enable an extension by ID                            |
| `extensions_disable`     | Disable an extension by ID                           |
| `extensions_add`         | Add a user extension (opens folder picker dialog)    |
| `extensions_remove`      | Remove a user extension by ID                        |
| `extensions_open_folder` | Open the user extensions directory in file manager   |
| `extensions_reload`      | Re-scan and reload user extensions                   |
| `adblock_toggle`         | Enable/disable the ad-blocker (persisted)            |
| `adblock_status`         | Query current ad-blocker on/off state                |
| `adblock_check`          | Check if a URL should be blocked                     |
| `darkreader_toggle`      | Enable/disable dark-reader (persisted)               |
| `darkreader_status`      | Query current dark-reader on/off state               |
| `darkreader_get_css`     | Retrieve the dark-reader CSS string for injection     |
| `page_info_get`          | Get page info (title, URL, description, counts)      |
| `fedgram_extract`        | Extract Instagram image URLs from a post URL         |
| `fedgram_download`       | Download an image (opens save dialog, writes to disk)|

---

## Extension System

Built-in extensions are bundled as Tauri resources (in
`src-tauri/extensions/builtin/`) and registered at startup.  Their original
manifests and assets are preserved, but the runtime logic is implemented
natively in Rust.

User extensions can be added via the Extensions Manager panel (puzzle icon in
the toolbar).  The folder picker (`tauri-plugin-dialog`) lets you select an
extension directory, which is copied to the user data directory and registered.
Extension state (enabled/disabled, ad-blocker toggle, dark-reader toggle) is
persisted in a `state.json` file in the app's data directory.

---

## Configuration

### `tauri.conf.json` highlights
- **Frameless window**: `decorations: false` — the app uses a custom title bar
  with minimize/maximize/close buttons and a draggable region.
- **CSP**: Configured to allow `frame-src https: http:` so external sites can
  be loaded in iframes.  `connect-src` includes `ipc:` for Tauri command
  invocation.
- **Bundle resources**: `extensions/builtin/**/*` are bundled so the Rust
  backend can access `rules.json` and other assets at runtime.

### Capabilities (`capabilities/default.json`)
Tauri v2's security model requires explicit permission grants.  The default
capability set includes:
- Core window operations (minimize, maximize, close, start-dragging)
- Webview defaults
- Dialog: `allow-open` (folder/file pickers)
- Shell: `allow-open` (open URLs in default browser, open file manager)
- Filesystem: scoped read/write/mkdir/read-dir/remove/copy

---

## License

This project preserves the licensing of the original Surf FED and the
FED-GRAM component (see `src-tauri/extensions/builtin/fed-gram/original/` for
the original FED-GRAM license and attribution).
