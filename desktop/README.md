# Tylluan Desktop

**Status: real integration in progress, not just a spike anymore.** Confirmed working: native
window opens the actual Tylluan dashboard, native chrome (minimize/maximize/resize/close), a
Reload menu item (Cmd/Ctrl+R). This is the direction going forward for a native Tylluan app.

## What this is

A Tauri v2 shell that opens a native window pointing at Tylluan's existing web dashboard
(`http://127.0.0.1:4000`, the same one served by `tylluan-nexus`). No dashboard UI was rewritten
— Tauri only provides the native chrome (window, title bar, menu, native dialogs) around what
already exists.

## Why Tauri, not Electron

Claude Desktop and Codex Desktop (both official) are built with Electron — but that bundles a
full Chromium per app (150-200MB binary, 200-400MB idle RAM), which directly contradicts
Tylluan's own promise of running on a Raspberry Pi 4. Tauri uses the OS's native WebView
(WebView2 / WebKit / WebKitGTK) and a Rust backend — same language as the kernel, no new
runtime to maintain. See `docs/roadmap/ROADMAP_O3.md` (Tylluan Desktop section) and
`CHANGELOG.md` for the full writeup and the sources behind this decision.

## Structure

- `src-tauri/` — the Rust/Tauri shell. **Its own Cargo workspace**, deliberately decoupled from
  the kernel workspace at the repo root (`../../Cargo.toml`) — this stays out of the kernel's
  build/lint/test gates until it's an actual release decision.
- That's it. The starter template ([dannysmith/tauri-template](https://github.com/dannysmith/tauri-template))
  this was scaffolded from shipped its own React frontend (quick-pane popup, preferences dialog,
  command palette, i18n, etc.) — all removed (2026-07-30): none of it was ever reachable, since
  `tauri.conf.json`'s `devUrl` points straight at the real dashboard instead of building that
  frontend, and it dragged in a real bug (referencing a `quick-pane.html` that was never even
  copied into this repo). Removing it also let several now-unused dependencies go (`serde`,
  `tauri-specta`/`specta`, `regex`, `tauri-nspanel`, `tauri-plugin-global-shortcut`,
  `tauri-plugin-process`, `tauri-plugin-updater`).

## Try it

1. Make sure the Tylluan kernel is running (`tylluan-cli start` or `cargo run -p tylluan-cli --
   start` from the repo root) and reachable at `http://127.0.0.1:4000`.
2. From this directory:
   ```bash
   cd src-tauri
   cargo tauri dev
   ```
   (requires `cargo install tauri-cli --version "^2.0.0"` once, if not already installed)

## What's verified so far

- Rust/Tauri backend compiles cleanly on Windows, no warnings (2026-07-30, post-cleanup).
- Native window opens with the real dashboard loaded, native chrome (minimize/maximize/resize),
  confirmed live on Windows.
- Reload menu item (Cmd/Ctrl+R) works.

## Next steps (see roadmap for the full plan)

- Confirm SSE/WebSocket reconnects and auth headers behave identically to the browser version.
- Embed a Monaco-based code editor alongside Coloquio (`suren-atoyan/monaco-react`, referencing
  `TimSusa/montauri-editor` for a working Tauri+Monaco example and `xuchaoqian/tauri-monaco-demo`
  for a known integration bug to watch for).
- Use `@tauri-apps/plugin-dialog` (already a dependency here) for the native folder picker in the
  planned sandbox permission system (per-folder off/on/ask).
- Design a real native title bar/menu that matches the dashboard's own design direction
  (see `dashboard/DESIGN_AUDIT.md`, "Nocturnal Observatory") instead of the bare OS default.
