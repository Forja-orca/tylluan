# Tylluan Desktop — feasibility spike

**Status: early feasibility spike, not a real product yet.** Confirms the technical direction
works; nothing here is production code.

## What this is

A Tauri v2 shell that opens a native window pointing at Tylluan's existing web dashboard
(`http://127.0.0.1:4000`, the same one served by `tylluan-nexus`). No dashboard UI was rewritten
— Tauri only provides the native chrome (window, title bar, system tray, native dialogs) around
what already exists.

## Why Tauri, not Electron

Claude Desktop and Codex Desktop (both official) are built with Electron — but that bundles a
full Chromium per app (150-200MB binary, 200-400MB idle RAM), which directly contradicts
Tylluan's own promise of running on a Raspberry Pi 4. Tauri uses the OS's native WebView
(WebView2 / WebKit / WebKitGTK) and a Rust backend — same language as the kernel, no new
runtime to maintain. See `docs/roadmap/ROADMAP_O3.md` (Tylluan Desktop section) and
`CHANGELOG.md` for the full writeup and the sources behind this decision.

## Structure

- `src-tauri/` — the Rust/Tauri shell. **Its own Cargo workspace**, deliberately decoupled from
  the kernel workspace at the repo root (`../../Cargo.toml`) — this spike is not part of the
  kernel's build/lint/test gates.
- `src/`, `index.html`, `vite.config.ts` — placeholder frontend scaffold inherited from the
  starter template ([dannysmith/tauri-template](https://github.com/dannysmith/tauri-template));
  currently unused since `tauri.conf.json`'s `devUrl` points straight at the real dashboard
  instead of building this frontend. Kept for now in case a native-only settings screen (e.g. the
  sandbox folder-permission picker) ends up living outside the web dashboard.

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

- The Rust/Tauri backend compiles cleanly on Windows with the system WebView2 (2026-07-30).
- Not yet verified: actually opening the window and confirming the real dashboard renders
  correctly inside it, on any platform.

## Next steps (see roadmap for the full plan)

- Confirm the window actually opens and the dashboard renders/functions identically to the
  browser version (SSE, WebSocket reconnects, auth headers).
- Embed a Monaco-based code editor alongside Coloquio (`suren-atoyan/monaco-react`, referencing
  `TimSusa/montauri-editor` for a working Tauri+Monaco example and `xuchaoqian/tauri-monaco-demo`
  for a known integration bug to watch for).
- Use `@tauri-apps/plugin-dialog` (already a dependency here) for the native folder picker in the
  planned sandbox permission system (per-folder off/on/ask).
