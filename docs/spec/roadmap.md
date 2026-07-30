# Photrez Roadmap

> Living document. Follows [SemVer](https://semver.org/) + standard release stages.
> Timelines are **estimates, not commitments** — they shift as scope and findings evolve.
> MVP v1 scope is locked in `product-scope.md`.

## Vision

A lightweight, fast desktop image editor that content creators and small businesses reach for daily — familiar editing flow, native performance, no subscription.

---

## Current Status

- **Version:** `v0.1.0-alpha.1` (2026-07-19)
- **MVP v1:** Complete — layer system, selection/transform, crop/resize, brush/eraser, export. See `FEATURES.md`.
- **Platform:** Windows-only.
- **Engine:** TypeScript `DocumentEngine`. Rust `photrez-core` is the domain-model reference + test coverage; WASM compute not yet wired.
- **Windowing:** Single main window, in-app document tabs. No detached panels, no multi-document windows.
- **Tests:** 2499 frontend + 113 Rust cases (incl. Playwright E2E).
- **Installer:** ~4-6 MB (well below 80 MB target).

---

## Release Pipeline

```
alpha.1 (DONE) → alpha.2 → beta.1 → beta.2 → beta.3 → rc → v1.0.0 (stable)
```

| Stage | Theme | User Outcome | Target |
| --- | --- | --- | --- |
| `alpha.2` | Stability, polish, small tools + WASM pilot | Fewer bugs, gradient/fill/ellipse; export runs faster (WASM proven) | ~Q3 2026 |
| `beta.1` | Floating panels + blend modes | Tear-off panels, full blend modes | ~Q4 2026 |
| `beta.2` | Multi-image windows + WASM extension | Each image in its own window; brush/transform faster via WASM | ~Q1 2027 |
| `beta.3` | Text & shapes | Text layers, shape drawing — daily-driver tools for content creators | ~Q2 2027 |
| `rc` | Cross-platform + perf gate | Works on Linux, macOS, Windows; all budgets met | ~Q3 2027 |
| `v1.0.0` | Stable daily-driver | Backward-compatible, production-ready on 3 OS | ~Q4 2027 |

---

## alpha.2 — Stability, Polish, Small Tools & WASM Pilot

> **Target:** ~Q3 2026 &nbsp;|&nbsp; **Confidence:** High

**User sees:** A noticeably more stable and polished editor. Known alpha.1 bugs fixed, startup time improved, three new tools (gradient, fill, elliptical selection), and export runs faster thanks to the first WASM module.

### Stability & Polish

- Fix all items in `KNOWN_ISSUES.md` that are marked "Fix planned: Beta" where feasible early (window state flash, titlebar accessibility).
- Startup time optimization (lazy loading, shader precompilation) — target ≤2.5s.
- ✅ UX polish: remaining silent-error paths resolved (merge, flatten, fill toasts), UI consistency, keyboard shortcut gaps.
- ✅ `.ptz` format stability hardening (migration path for alpha→beta documented in `docs/guide/ptz-migration.md`).

### New Tools

- **Elliptical selection:** Ellipse marquee alongside existing rectangle — same modifier keys (Shift = circle, Alt = center-out), same operations (move, invert, cut/copy/paste/delete). Resolves KNOWN_ISSUES #4.
- **Gradient tool:** Linear and radial gradient fill on active layer or selection. Foreground-to-background and foreground-to-transparent presets.
- **Paint bucket (fill tool):** Click-to-fill contiguous area with foreground color. Tolerance slider. Supplements existing Alt+Delete shortcut (which fills the whole layer/selection).

### WASM Pilot (De-risk Early)

- **Goal:** Prove the `wasm-pack` + Vite + SolidJS build pipeline with one isolated, easy-to-validate hot-path.
- **Target:** Export encode — `photrez-core::export` compiled to WASM, called zero-copy from TS (`Uint8Array`). Parity tests vs current TS implementation.
- **Why export first:** Isolated (no state dependency), easy to validate (byte-for-byte output comparison), measurable speedup (encode is CPU-bound).
- **Success criteria:** WASM export produces identical output to TS export; build pipeline stable; no dev-experience regression.

### Definition of Done

- All P0 known issues resolved or mitigated.
- Startup cold-launch ≤ 2.5s on target hardware (4GB RAM + SSD).
- Elliptical selection: draw, move, invert, cut/copy/paste/delete — parity with rectangle.
- Gradient tool: linear + radial, selection-aware, undoable, option bar with type/direction controls.
- Paint bucket: tolerance-based fill, selection-aware, undoable.
- WASM export encode: functional, parity-tested, build pipeline documented.
- All existing tests pass; no regression.

---

## beta.1 — Floating Panels + Blend Modes

> **Target:** ~Q4 2026 &nbsp;|&nbsp; **Confidence:** Medium

**User sees:** Tear off panels (Layers, Inspector) into separate floating windows and re-dock them. All blend modes unlocked.

### Architecture (required before any UI)

- Tauri multi-`WebviewWindow` spawn + close lifecycle.
- Lightweight state-sync bridge between webviews using Tauri events with a **revision gate** (single source of truth, no per-property `invoke` chatter).
- Keep document state in the TypeScript `DocumentEngine` (per webview owner model). Do NOT move document state into Rust `tauri::State`.
- WASM compute stays in its webview of origin; the bridge carries only lightweight state (selection, active doc, undo index, preferences), never pixel buffers.

### Features

- **Detachable panels:** Layers panel can tear off into a native floating window and re-dock. Panel window shares the Solid store via the event bridge (view + input only; main webview remains owner).
- **Full blend modes:** Unlock remaining blend modes already in shader (Multiply, Screen, Overlay are in alpha; add Darken, Lighten, Color Dodge, Color Burn, Soft Light, Hard Light, Difference, Exclusion) once WebGL preview / Canvas2D export parity tests pass.

### Definition of Done

- Window spawn/close lifecycle verified.
- Layers panel detaches/re-docks without state loss.
- Bridge sync test proves isolation + revision-gate correctness.
- All unlocked blend modes render correctly in both preview AND export.

### Known Risks

- Tauri multi-window is relatively young — state sync edge cases may require workarounds.
- Event bridge latency could cause visible lag on slow machines.

---

## beta.2 — Multi-Image Windows + WASM Extension

> **Target:** ~Q1 2027 &nbsp;|&nbsp; **Confidence:** Medium-Low

**User sees:** Open each image in its own native window. Drag layers/tabs between windows. Brush strokes and transforms run noticeably faster.

### Architecture

- Each document opens in its own webview that owns its `DocumentEngine` + WASM compute (no single-webview bottleneck).
- Thin bridge between document webviews for cross-document actions (drag layer between docs, shared undo list).

### WASM Extension (build pipeline proven in alpha.2)

- Port editing hot-paths from TypeScript to `photrez-core` WASM:
  - **Brush mask generation** — CPU-bound dab computation.
  - **Transform matrix math** — scale/rotate/flip pixel operations.
  - **Tile split/compose** — large canvas tiling operations.
- Each port: zero-copy call from TS (`Uint8Array`), parity tests vs TS implementation.

### Features

- **Multi-document windows:** Open several images in separate native windows; drag layers/tabs between them.
- **History panel** *(scope-approval needed)*: Visual undo/redo list panel (engine already exists, UI only).

### Definition of Done

- Two documents open in separate windows; editing in each is independent and fast.
- At least two additional hot-paths run via WASM with parity tests vs TS implementation.
- History panel (if approved): clickable list, thumbnails, matches engine undo stack.

### Known Risks

- Cross-document drag requires careful ownership transfer and undo semantics.
- WASM brush mask may need different memory management patterns than export encode.

---

## beta.3 — Text & Shapes

> **Target:** ~Q2 2027 &nbsp;|&nbsp; **Confidence:** Medium-Low

**User sees:** Add text to images — titles, captions, watermarks. Draw shapes for annotations and design elements. These are the tools that make Photrez a real daily-driver for content creators.

### Features

- **Text tool:** Click to place text, type to edit. Font family, size, color, alignment, bold/italic. Text lives on its own layer (non-destructive — editable until rasterized). Rasterize command bakes text to pixels.
- **Shape tool:** Rectangle, ellipse, line, arrow. Fill and/or stroke with foreground/background color. Shape lives on its own layer (vector until rasterized). Option bar: shape type, fill/stroke toggle, stroke width.

### Architecture Notes

- Text and shape layers extend the `LayerType` union in `DocumentEngine` — they carry metadata (font, shape params) alongside or instead of a pixel buffer.
- Rendering: WebGL2 rasterizes text/shape to a temporary texture for compositing (same pipeline as pixel layers). No new renderer required.
- `.ptz` format extension: text/shape layer metadata stored in `document.json`; rasterized fallback bitmap stored alongside for forward-compatibility.

### Definition of Done

- Text tool: place, edit, style (font/size/color/alignment/bold/italic), move, transform, rasterize, undo/redo. Option bar complete.
- Shape tool: rectangle, ellipse, line, arrow. Fill + stroke. Move, transform, rasterize, undo/redo. Option bar complete.
- Both tool types: export correctly (rasterized to output), saved/loaded in `.ptz`.
- Keyboard shortcut: T (text), U (shape).

### Known Risks

- Text rendering cross-platform consistency (font availability, rendering differences between Windows/Linux/macOS) — mitigated by deferring cross-platform to rc.
- `.ptz` format extension must be backward-compatible (older Photrez reads the fallback bitmap, ignores text/shape metadata it doesn't understand).

---

## rc — Cross-Platform + Perf Gate + Polish

> **Target:** ~Q3 2027 &nbsp;|&nbsp; **Confidence:** Low (depends on beta outcomes)

**User sees:** Photrez works on Linux, macOS, and Windows. Feels fast and stable. No new features — everything is about reliability.

### Work

- Linux + macOS build/run validation (today Windows-only).
- Platform-specific testing: native menus, file dialogs, window management, HiDPI, font rendering, keyboard shortcuts, text tool font availability.
- **Perf gate** (all 3 OS):
  - Installer < 80 MB
  - Idle RAM < 250 MB
  - Startup < 2s
- Accessibility audit: keyboard navigation, screen reader labels, contrast.
- Bug-fix only — no new features.
- Documentation: user guide + alpha→stable migration note.
- `.ptz` format freeze: backward-compat from this point forward.

### Known Risks

- Linux window management varies widely (X11 vs Wayland, tiling WMs).
- macOS Gatekeeper / notarization adds build pipeline complexity.
- HiDPI behavior differs across platforms (especially mixed-DPI setups).
- Text rendering consistency across OS (font fallback chains).

---

## v1.0.0 — Stable Release

> **Target:** ~Q4 2027 &nbsp;|&nbsp; **Confidence:** Low (depends on rc outcomes)

**User sees:** A reliable, daily-driver image editor on Windows, Linux, and macOS.

### What "Daily-Driver Ready" Means

- All MVP features + text + shapes + blend modes + gradient/fill + elliptical selection work reliably on 3 OS without data loss.
- Multi-window workflow is stable (floating panels, per-document windows).
- `.ptz` format is stable — files saved in v1.0 will open in future versions.
- Performance budgets met on all platforms.
- WASM compute is mature for export encoding and at least two editing hot-paths.
- No P0 or P1 known issues.

### What "Backward-Compatible" Means

- SemVer MAJOR stable: breaking changes require a v2.0.
- `.ptz` format locked — old files always openable.
- Keyboard shortcuts and UI layout stable (changes only via opt-in).

---

## Out of Scope (v1 Cycle)

The following are explicitly excluded from the alpha→v1.0 pipeline. They may be considered post-v1.

- PSD open/save workflow.
- Full ICC color management engine & soft-proofing (Pro Print Settings UI is in alpha.2; ICC profiles post-v1).
- Plugin/scripting runtime or API.
- AI-powered editing features.
- Cloud collaboration or sync.
- Command palette.
- Advanced retouching (spot healing, healing brush, clone stamp, red-eye).

Reference: `product-scope.md`

---

## Future Considerations (Post-v1)

> **Not committed.** These are possibilities under consideration for the v2+ cycle, listed to provide transparency — not promises.

| Area | Possibility |
| --- | --- |
| Selection | Lasso (freehand), magic wand (color-based), feathered edges |
| Adjustments | Non-destructive adjustment layers (Curves, Levels, Hue/Saturation) |
| Retouching | Clone stamp, healing brush, dodge/burn/sponge |
| Touch-up | Blur, sharpen, smudge brush |
| Drawing | Pen tool / vector paths |
| Extensibility | Plugin SDK / scripting API |
| Formats | PSD import/export, `.ptz` v2 format |
| Platform | Command palette, touch/tablet pressure dynamics |
| Performance | GPU compute via WebGPU (when stable in Tauri) |

---

## Architecture Invariants

These hold across the entire v1 roadmap and are consistent with `ARCHITECTURE.md`:

1. **Document state stays in TypeScript.** The `DocumentEngine` owns document truth. Rust WASM modules are called for compute, not ownership — no dual-state sync.
2. **WebGL2 stays as the renderer.** Compositing is already ~2ms at 4K. wgpu is deferred until compute shaders or advanced blend modes exceed WebGL2 capabilities.
3. **Tauri IPC is cold-path only.** File I/O and system dialogs. Hot-path operations (brush, transform, export) stay in the webview (TS today, WASM tomorrow).
4. **Multi-window is Tauri windows + event bridge.** WASM accelerates editing inside each webview; it does not create or manage windows.

---

## Contributing

Interested in contributing? See `CONTRIBUTING.md` for guidelines. Areas where help is especially welcome are marked in the GitHub issue tracker with the `help-wanted` label.
