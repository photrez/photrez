# Known Issues — Photrez v0.1.0-alpha.2

This document lists known limitations and bugs in the current alpha release.
For bug reports, please open an issue at https://github.com/photrez/photrez/issues.

## 🚨 Critical Limitations

### 1. Startup Time
- **Symptom (resolved in alpha.2):** Earlier estimate cited ~3.7s cold launch. On the installed Windows build the app launches near-instantly; frontend mount measures ~37ms (Solid + WebGL2 init). The previous 3.7s was a stale dev-first-run estimate, not representative of release builds.
- **Fix (alpha.2):** Removed the splashscreen window — the app now shows the main window directly (Tauri guidance: splashscreen only masks slow loads; Photrez is fast enough that it only added a black flash). `main` window is visible by default with a solid editor background, so there is no white/black flash.
- **Status:** No startup optimization needed. Target (<2s) met in practice.

### 2. Windows-Only
- **Symptom:** No macOS or Linux builds available for alpha.
- **Workaround:** Use Windows 10/11. For Mac/Linux, build from source (untested).
- **Fix planned:** Beta release (cross-platform CI matrix).

## 🎨 Editor Limitations

### 3. Blend Modes
- **Symptom:** Only Normal, Multiply, Screen, Overlay are available in UI.
- **Reason:** Other modes exist in shader but are blocked pending WebGL preview / Canvas2D export parity tests.
- **Fix planned:** 0.2.0 (post-MVP; roadmap moved full blend modes out of the pre-1.0 MVP pipeline).

### 4. Selection Tools
- **Symptom:** Rectangular + elliptical marquee only.
- **Shipped (alpha.2):** Elliptical marquee (drag draws ellipse; copy/cut/delete scope to the ellipse; Rect/Ellipse toggle + M / Shift+M).
- **Not available:** Lasso, magic wand.
- **MVP scope:** Rectangular + elliptical. Advanced selection is post-v1.0.

### 5. Brush Engine
- **Symptom:** Only round brush tip with hardness/flow/smoothing.
- **Not available:** Custom brush shapes, texture brushes, dynamics (pressure/tilt/velocity).
- **MVP scope:** Round tip only.

### 6. High-Zoom Pixelation
- **Symptom:** At zoom levels where canvas backing buffer would exceed 4096px, preview becomes pixelated.
- **Reason:** 4096px clamp to prevent WebGL context loss on low-VRAM GPUs.
- **Workaround:** Pan to inspect different regions instead of extreme zoom.

### 7. Large Layer Bake Hitch
- **Symptom:** First "Apply & Paint" on a layer with active adjustments may cause a brief hitch (50-200ms) for large layers (4K+).
- **Reason:** Synchronous `gl.readPixels` for GPU→CPU transfer.
- **Fix planned:** v1.0 (PBO-based async readback).

## 💾 File Format

### 8. PSD Not Supported
- **Symptom:** Cannot open or export `.psd` files.
- **MVP scope:** PSD is non-goal. Use PNG/JPEG/WebP for export.

### 9. Project Format (`.ptz`) Stabilization
- **Symptom:** `.ptz` format is not stable. Future versions may break compatibility.
- **Workaround:** Export to PNG/JPEG/WebP for long-term storage.
- **Stabilization:** Backward compatibility is guaranteed starting with the first stable release (`v0.1.0`): all subsequent format changes must be additive and ship with a migrator (`docs/guide/ptz-migration.md`). `v1.0.0` marks the final format lock.

## 🔧 Development Limitations

### 10. No Plugin/Scripting API
- **Symptom:** No plugin or scripting support.
- **MVP scope:** Non-goal. Plugin SDK is post-v1.0.

### 11. No Cloud Sync
- **Symptom:** No cloud sync or collaboration features.
- **MVP scope:** Non-goal. Local-only editing.

### 12. Autosave is Local Only
- **Symptom:** Autosave writes to local app config directory. No cloud backup.
- **Workaround:** Manually save to cloud-synced folder (OneDrive, Google Drive).

## 🐛 Known Bugs

### 13. Window State Restore on Multi-Monitor
- **Symptom (mitigated in alpha.2):** If saved window position is on a disconnected external monitor, app snaps to primary monitor center (intended). The earlier "brief flash at default size" was caused by the splashscreen restore race; removing the splashscreen (alpha.2) eliminates that flash. A minor resize flash on main-window restore may still occur on multi-monitor and is low priority.
- **Fix planned:** Beta release (if still observable after splashscreen removal).

### 14. Custom Titlebar Accessibility
- **Symptom:** Custom titlebar may not fully support keyboard navigation (Alt+Space system menu, F10 menu activation).
- **Mitigation (code evidence, 2026-08-02):** Native menu bar is installed via `app.set_menu` (`main.rs:125`) with `on_menu_event` routing to the frontend — Windows handles F10/Alt+Space activation for windows with a native menu bar. Workaround: standard keyboard shortcuts (Ctrl+N, Ctrl+O, etc.).
- **Status:** Verify manually during the beta.1 regression pass; fix only if F10/Alt+Space still fail on a release build.

### 15. CSP Allows `unsafe-inline` for Styles
- **Symptom:** Content Security Policy allows inline styles (required for Tailwind CSS v4).
- **Risk:** Low — does not affect scripts. CSS injection is limited risk.
- **Fix planned:** v1.0 (hash-based CSP).

## 📊 Performance Notes

### 16. Idle RAM: ~34 MB — well below 250 MB target.
### 17. Installer Size: 4-6 MB — well below 80 MB target.
### 18. Test Coverage: 2683 frontend + 171 Rust cases (incl. Playwright E2E).

## 🔄 Migration Path

### From v0.1.0-alpha.2 to v0.1.0 (first stable)
- `.ptz` backward-compat is guaranteed from `v0.1.0`: all later format changes are additive and ship a migrator (`docs/guide/ptz-migration.md`). Files saved in alpha/beta stay loadable.
- Settings and window state will migrate automatically.
- Breaking changes may still land during `0.x` MINOR bumps (per SemVer) — always documented in release notes.

## 📞 Reporting Issues

- **Bug reports:** https://github.com/photrez/photrez/issues
- **Security reports:** See `SECURITY.md` (report privately before public disclosure)
- **Feature requests:** Open a discussion or issue with `feature-request` label

Thank you for testing Photrez alpha! Your feedback helps shape the 0.1.0 stable and 1.0 releases.
