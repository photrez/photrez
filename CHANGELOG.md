# Changelog

All notable changes to Photrez will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project aims to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once stable releases begin.

## [Unreleased]

(no unreleased changes yet)

---

## [0.1.0-alpha.2] — 2026-08-01

### ⚠️ Pre-Release Notice

This is an **alpha release** for early testing and feedback. Expect bugs,
breaking changes, and incomplete features. Not recommended for production use.

### ✨ Added

- Elliptical marquee selection — drag draws an ellipse; copy/cut/delete scope to the ellipse interior; Rect/Ellipse toggle in the option bar (M / Shift+M).
- Gradient tool — linear + radial, selection-aware, undoable, option bar with type/direction controls.
- Paint bucket tool — tolerance-based fill, selection-aware, undoable.
- WASM export encode pilot — `photrez-core` PNG/JPEG/WebP encoders compiled to WebAssembly via `wasm-pack`, wired zero-copy into `exportDocument.ts` with automatic Canvas fallback (parity-tested vs the TypeScript encoder).
- Move tool improvements — canvas-edge snap boost, locked-layer fallthrough, alpha-aware layer hit-test (transparent pixels pass through to the layer underneath).
- Eyedropper option bar — HEX readout, copy, auto-copy to clipboard.
- Print pipeline performance phases 1–3 — native-DPI 1:1 composite, raw `Uint8Array` IPC (no temp file / base64), and a raw-RGBA zero-encode path to GDI.
- No-layer UX guards — brush/eraser/fill/delete/flip/duplicate/merge/flatten/apply-adjustment now surface a warning toast instead of silently no-op'ing.

### 🎨 Changed

- Splashscreen window removed — the main window shows directly, eliminating the black flash and reducing startup latency; window-state restore now snaps to the primary monitor if the saved position is off-screen.
- Brush stroke smoothing via coalesced pointer events (stylus / high-Hz pointers produce smooth curves); brush cursor previews the active-layer transform (ellipse + rotation).
- Crisp pixel editing above 200% zoom — renderer uses `NEAREST` magnification to avoid bilinear blur.
- History VRAM disposal — evicted/cleared undo snapshots release their `ImageBitmap`s (no GPU memory leak on long sessions).
- Parallel batch-open for multi-file import; Alt+resize no longer drifts the center; transform resize keeps aspect ratio ON by default (Shift inverts to free-resize).
- Adjustments sliders debounced (100ms) — slider leads, engine follows; hardcoded accent colors moved to CSS variables.
- Six silent `console.error` paths converted to `showToast`; option-bar labels standardized.

### 🐛 Fixed

- Background layer no longer reorderable (was reachable via drag).
- Brush stroke anchor now restored deterministically on undo.
- Selection move no longer commits ghost history entries on click-only drags.
- Close button resolution in production webview — dynamic `@vite-ignore` import replaced with a static import.

### 🧪 Testing

- 2683 frontend test cases / 169 files, Rust 171 cases (84 core + 87 desktop); type-check and build green.
- Test hygiene: `mockUseEditor` helper + typed Tauri/brush/drag mocks (`as any` in tests reduced 483 → 361).

### 📦 Distribution

- Windows MSI / NSIS installer; tagged `v0.1.0-alpha.2` as a GitHub pre-release.

---

## [0.1.0-alpha.1] — 2026-07-19

### ⚠️ Pre-Release Notice

This is an **alpha release** for early testing and feedback. Expect bugs,
breaking changes, and incomplete features. Not recommended for production use.

### ✨ Added

- Tauri 2 desktop shell with custom title bar, native menu, dialogs, file open/export flows.
- SolidJS editor UI: tool rail, document tabs, canvas viewport, inspector, layers panel, history panel, navigator, context menus, status bar.
- Multi-document workspace with cross-document drag-and-drop.
- Layer operations: create, duplicate, delete, reorder, visibility, lock, opacity, merge down, flatten, blend modes (Normal/Multiply/Screen/Overlay), basic adjustments (brightness/contrast/saturation, non-destructive).
- Selection: rectangular marquee, invert, cut/copy/paste/delete, move, rotate.
- Transform: scale, rotate, flip, snapping, keyboard nudges, aspect-ratio constraint.
- Crop: classic and modern modes, aspect-ratio presets, canvas expansion.
- Brush and eraser: calibrated round-tip with hardness, flow, smoothing, presets.
- Eyedropper, color picker, fill shortcuts.
- Export: PNG, JPEG, WebP with quality settings.
- Native print dialog integration (Windows).
- History: undo/redo with VRAM-aware disposal.
- Save/load project format (`.ptz` — zip archive with document.json + layer PNGs).
- Window state persistence across launches.
- WebGL2 renderer with context-loss recovery.
- Rust `photrez-core` domain crate (reference implementation).
- Automated frontend tests (2499 cases), Rust tests (113: 84 core + 29 desktop), Playwright E2E specs.
- Public documentation: architecture, features, PRD, TRD, design system, contributing, security, governance.
- Help ▸ About dialog shows the current version dynamically (read from `tauri.conf.json` via Tauri `getVersion()`).

### 🐛 Known Issues

See `KNOWN_ISSUES.md` for the full list. Highlights:

- Startup time on cold launch is ~3.7s (target: <2s). Optimization planned for beta.
- High-zoom canvas preview may appear pixelated due to 4096px backing-buffer clamp.
- WebGL2 `readPixels` is synchronous — brief hitch possible on first "Apply & Paint" for large layers.
- macOS and Linux are not yet tested in CI — Windows is the only supported platform for alpha.
- Blend modes beyond Normal/Multiply/Screen/Overlay are blocked from UI (parity tests pending).
- Selection is rectangular only (lasso/magic wand not in MVP scope).
- PSD import is not supported (non-goal for MVP).

### 🔒 Security

- Path-traversal validation: canonicalize + symlink check + extension allowlist (`validate_path_safe`).
- `delete_file` restricted to the OS temp directory.
- `print_image` allowlist narrowed to PNG/JPEG (PDF dropped — non-MVP).
- File-size cap: 256 MB per IPC operation.
- Project file (`.ptz`) zip-bomb protection: per-entry decompressed size limit.
- CSP enabled with `script-src 'self'` (no inline scripts).

### 📦 Distribution

- Windows MSI / NSIS installer (built via `bun run tauri build`).
- Available on GitHub Releases (pre-release flag enabled).

### 🔗 Links

- Source: https://github.com/rahmanqolbi/photrez
- Issues: https://github.com/rahmanqolbi/photrez/issues
- Security: see SECURITY.md
