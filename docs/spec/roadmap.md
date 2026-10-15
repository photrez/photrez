# Photrez Roadmap

> Living document. Follows [SemVer](https://semver.org/) + standard release stages.
> Timelines are **estimates, not commitments** — they shift as scope and findings evolve.
> MVP v1 scope is locked in `product-scope.md`.

## Vision

A lightweight, fast desktop image editor that content creators and small businesses reach for daily — familiar editing flow, native performance, no subscription.

---

## Current Status

- **Version:** `v0.1.0` (2026-08-06) — first stable release; `0.2.0` is in development.
- **MVP v1:** Complete — layer system, selection/transform, crop/resize, brush/eraser, gradient/fill, elliptical selection, export. See `FEATURES.md`.
- **In `main` for the planned 0.2.0 release:** text layers, parametric shape layers, and all 12 W3C blend modes.
- **Platform:** Windows primary (macOS/Linux pre-release builds available, not yet fully QA'd).
- **Engine:** Rust `photrez-core` is the canonical document/pixel/history authority; the TypeScript UI keeps a read-only projected view.
- **Current shell:** Tauri 2 + SolidJS/TypeScript + WebGL2. The proposed 0.2.0 native Rust UI is not yet implemented or shipped.
- **Windowing:** Single main window, in-app document tabs. No detached panels, no multi-document windows.
- **Tests:** Latest recorded frontend run (2026-10-14): 393 files / 4,465 passed, 2 expected failures, 1 skipped. Rust core and desktop: 354 / 114 passing tests (2026-09-18).
- **Installer:** Current v0.1.0 installer ~4-6 MB. Native-build size will be measured before release; no native size claim yet.

## Current Status

- **Version:** `v0.1.0` (2026-08-06) — first stable release; `0.2.0` is in development.
- **MVP v1:** Complete — layer system, selection/transform, crop/resize, brush/eraser, gradient/fill, elliptical selection, export. See `FEATURES.md`.
- **In `main` for the planned 0.2.0 release:** text layers, parametric shape layers, and all 12 W3C blend modes.
- **Current shell:** Tauri 2 + SolidJS/TypeScript + WebGL2. The planned native Rust UI shell is not yet implemented or shipped.
- **Engine:** Rust `photrez-core` owns canonical document, pixel, and history state; the TypeScript UI keeps a derived view.
- **Platform:** Windows primary (macOS/Linux pre-release builds available, not yet fully QA'd).
- **Windowing:** Single main window with in-app document tabs; detached panels and multi-document windows are planned.
- **Tests:** Latest recorded frontend run (2026-10-14): 393 files / 4,465 passed, 2 expected failures, 1 skipped. Rust core and desktop runs: 354 / 114 passed (2026-09-18).
- **Installer:** Current v0.1.0 installer is ~4–6 MB. Native runtime figures are not available yet.
---

## Release Pipeline

Stable releases ship per feature batch using SemVer MINOR releases. Alpha builds are internal; one public beta is published for feedback, followed by the stable release once its quality gates pass. A release candidate is used when beta testing identifies blockers. For `0.2.0`, text, shape, and blend-mode features are complete in `main`; the native Rust UI shell remains in development and is a release gate. It is not described as shipped until released.

```
alpha.1 (DONE, 2026-07-19) → alpha.2 (DONE, 2026-08-01) → 0.1.0-beta.1 (DONE, 2026-08-03)
  → 0.1.0 (STABLE MVP, DONE 2026-08-06)
  → 0.2.0 (The Native Update: text, shapes, all blend modes, native UI shell)
  → 0.3.0 (floating panels, planned)
  → 0.4.0 (multi-image windows, planned)
  → 1.0.0-rc → 1.0.0 (cross-platform + performance gate, planned)
```

| Version | Theme | User outcome | Target |
| --- | --- | --- | --- |
| `0.1.0-beta.1` | Final MVP polish | ✅ Released 2026-08-03 — MVP feature freeze; `.ptz` v1 began its compatibility line | Done |
| `0.1.0` | Stable MVP | ✅ Released 2026-08-06 — first stable release; `.ptz` backward compatibility begins | Done |
| `0.2.0` | **The Native Update** | Text layers, shape layers, all 12 blend modes, and planned native Rust UI shell | In development |
| `0.3.0` | Floating panels | Detachable Layers and Inspector panels | Planned |
| `0.4.0` | Multi-image windows | Multiple documents in separate native windows | Planned |
| `1.0.0-rc` | Cross-platform + performance gate | Windows, macOS, Linux; release budgets measured | Planned |
| `1.0.0` | Stable daily-driver | Backward-compatible projects; final `.ptz` format lock | Planned |

- **SemVer:** During `0.x`, minor releases may contain breaking changes; patch releases are for fixes. The release cycle and targets are listed here and in `CHANGELOG.md`.
- **Project files:** `.ptz` backward compatibility starts at `0.1.0`; format changes must be additive and include a migrator (`docs/guide/ptz-migration.md`). The final format lock is planned for `1.0.0`.
- **Command contract:** The current shell command contract is documented in `docs/reference/command-contract-spec.md`; its version is separate from the app version.

---

## alpha.2 — Stability, Polish, Small Tools & WASM Pilot

> **Status:** ✅ DONE — tagged `v0.1.0-alpha.2` (2026-08-01)
>
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

## 0.3.0 — Floating Panels

> **Target:** TBD &nbsp;|&nbsp; **Confidence:** Medium

**User sees:** Tear off panels (Layers, Inspector) into separate floating windows and re-dock them.

### Architecture (required before any UI)

- Built on the native UI shell planned for `0.2.0`; do not base this milestone on additional web-view windows.
- Panels are views onto the shared document state in the Rust core; no per-window document copy or cross-window state bridge.

### Features

- **Detachable panels:** Layers and Inspector can tear off into native floating windows and re-dock.

### Definition of Done

- Window spawn/close lifecycle verified.
- Panels detach and re-dock without state loss.
- Two views of the same document remain consistent after edits.

### Known Risks

- Floating-window behavior varies by platform; validate during the native release cycle.

---

## 0.4.0 — Multi-Image Windows

> **Target:** TBD &nbsp;|&nbsp; **Confidence:** Medium

**User sees:** Open each image in its own native window. Drag layers/tabs between windows.

### Architecture

- Each document window is a view over the shared Rust core; it does not own a duplicate engine.
- Cross-document actions operate on the same in-process document authority.

### Features

- **Multi-document windows:** Open several images in separate native windows; drag layers/tabs between them.
- **History panel** *(scope approval required)*: Visual undo/redo list panel.

### Definition of Done

- Two documents open in separate windows; editing in each is independent and reliable.
- History panel, if approved, is clickable, shows thumbnails, and reflects the canonical history stream.

### Known Risks

- Cross-document drag requires careful ownership transfer and undo semantics.

---

## 0.1.0-beta.1 — Final MVP Polish

> **Status:** ✅ DONE — tagged `v0.1.0-beta.1` (2026-08-03)

**User sees:** The MVP feature set frozen. No new features — only bug fixes, polish, and stability verification ahead of the first stable release.

### Work

- ✅ Feature-freeze the MVP scope (`product-scope.md`) — FROZEN 2026-08-02, effective `0.1.0-beta.1`.
- ✅ Resolve alpha.2 follow-ups: dependency audit (`bun audit` clean — 0 high, 2 moderate + 1 low astro pending upstream bump) + bundle hygiene (index chunk 485.7 kB, under the 500 kB budget) + contract spec §7 sync (`PUBLIC_CONTRACT_COMMANDS` subset).
- ✅ `.ptz` v1 locked for the MVP scope; document the migration path (`docs/guide/ptz-migration.md`).
- ✅ Regression pass over all MVP workflows (2026-08-02: 2683 frontend + 171 Rust + 25 E2E green); startup verified against the existing release-gate evidence (`KNOWN_ISSUES.md` #1 — <2s target met, no perf regression since alpha.2; bundle shrank 585 → 485.7 kB).
- ✅ Cross-platform pre-release builds (Windows NSIS, macOS DMG, Linux deb/rpm/AppImage) in the beta.1 release.
- ✅ Beta.1 follow-up fixes: move-tool drag hit-test aligned with selection at transparent pixels; print dialog body padding override (`!p-0`); Ctrl+P browser-print interference; eraser composite artifacts.

### Definition of Done

- ✅ No new features merged after this stage starts (bug-fix only).
- ✅ All tests green (2687 frontend + 84 Rust core); no P0 issues open.
- ⏳ Release-candidate quality verified on the Windows release build (pending user smoke test of the beta.1 installer).

---

## 0.2.0 — The Native Update (Text & Shapes + Blend Modes + Native UI Shell)

> **Status:** In development. Text, shape, and blend-mode features are complete in `main`; the native UI shell remains planned work and has not shipped.

**User sees:** Text layers for titles, captions, and watermarks; editable shapes for annotations and design elements; all 12 blend modes. The planned native shell replaces the shipped Tauri web-view UI with a native Rust interface. Do not claim native runtime performance or package size until measured on a release candidate.

### Features

- **Text tool:** Click to place text, type to edit; font family, size, color, alignment, bold/italic. Non-destructive text layers are editable until rasterized. **(complete in `main`; planned for 0.2.0)**
- **Shape tool:** Rectangle, ellipse, line, arrow, star; fill/stroke controls. Parametric shape layers are editable until rasterized. **(complete in `main`; planned for 0.2.0)**
- **Blend modes:** All 12 W3C modes, with preview/export parity verified. **(complete in `main`; planned for 0.2.0)**
- **Native UI shell:** Native Rust interface replaces the current Tauri web-view shell. **(planned; implementation and parity gates pending)**

### Architecture Notes

- Text and shape metadata are persisted in additive `.ptz` versions v2/v3; v3 is the current writer version and backward compatibility starts with v0.1.0 (`docs/guide/ptz-migration.md`).
- The Rust core is already the canonical document, pixel, and history authority in `main`; the UI holds a derived view. The native shell is a separate, not-yet-shipped runtime migration.

### Definition of Done

- Text/shape tools and all 12 blend modes are complete in `main`; release packaging and user validation remain part of the 0.2.0 release gate.
- The native shell reaches full feature parity with the shipped build, proven by the replacement harness and release-candidate testing.
- Native startup, frame pacing, memory, and installer size are measured on the release candidate and documented with the exact measurement commands.

### Known Risks

- Text rendering consistency depends on platform font availability and rasterization.
- `.ptz` remains backward-compatible from v0.1.0; new format changes must stay additive and ship a migrator.
- The native shell is a major runtime change. It remains a planned feature until implementation, parity, and release gates pass.

---

## 1.0.0-rc — Cross-Platform + Perf Gate + Polish

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
- `.ptz` finalization: format has been backward-compat since `0.1.0`; v1.0.0 freezes the format.

### Known Risks

- Linux window management varies widely (X11 vs Wayland, tiling WMs).
- macOS Gatekeeper / notarization adds build pipeline complexity.
- HiDPI behavior differs across platforms (especially mixed-DPI setups).
- Text rendering consistency across OS (font fallback chains).

---

## v1.0.0 — Stable Release

> **Target:** ~Q4 2027 &nbsp;|&nbsp; **Confidence:** Low (depends on rc outcomes)

**User sees:** A reliable, daily-driver image editor on Windows, Linux, and macOS.

### Definition of Done (measurable)

- All features through 0.4.0 working on Windows, Linux, and macOS without data loss.
- Perf gate green on all 3 OS: installer < 80 MB, idle RAM < 250 MB, startup < 2s.
- Every `.ptz` saved by 0.1.0 → 0.4.0 opens without error (backward-compat evidence incl. migrator tests).
- No P0/P1 known issues open.
- `contract_version` frozen across the rc cycle (see `command-contract-spec.md` §11).

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

The following are explicitly excluded from the pre-1.0 pipeline. They may be considered post-v1.

- PSD open/save workflow.
- Full ICC color management engine & soft-proofing (Pro Print Settings UI is in the MVP; ICC profiles post-v1).
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
| Formats | PSD import/export |
| Platform | Command palette, touch/tablet pressure dynamics |
| Performance | Native rendering and compute improvements, evaluated against measured release baselines |

---

## Architecture Invariants

These invariants describe the current app and the planned native shell without implying the migration is already complete:

1. **Rust owns persistent document truth.** The core is canonical for document state, pixels, and history; the current TypeScript UI retains only a derived view and transient interaction state.
2. **The current release renderer is WebGL2.** The planned native shell uses the toolkit-owned renderer; no performance claim is made until it is measured.
3. **Current shell operations use the Tauri command boundary where required.** The planned native shell will call the Rust core in-process; document edits must not reintroduce duplicate canonical state.
4. **Windowing is presentation-layer work.** Document logic remains independent of the number of windows or panels.

---

## Contributing

Interested in contributing? See `CONTRIBUTING.md` for guidelines. Areas where help is especially welcome are marked in the GitHub issue tracker with the `help-wanted` label.
