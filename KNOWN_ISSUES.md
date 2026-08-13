# Known Issues & Limitations — Photrez v0.1.0

This document tracks known limitations, performance notes, and planned features for the current release.
To report a bug, please open an issue at https://github.com/photrez/photrez/issues.

## 🖥️ Platform & Performance Notes

### 1. Startup Time
- **Symptom (resolved in alpha.2):** Earlier estimate cited ~3.7s cold launch. On the installed Windows build the app launches near-instantly; frontend mount measures ~37ms (Solid + WebGL2 init). The previous 3.7s was a stale dev-first-run estimate, not representative of release builds.
- **Fix (alpha.2):** Removed the splashscreen window — the app now shows the main window directly (Tauri guidance: splashscreen only masks slow loads; Photrez is fast enough that it only added a black flash). `main` window is visible by default with a solid editor background, so there is no white/black flash.
- **Status:** No startup optimization needed. Target (<2s) met in practice.

### 2. Cross-Platform Quality
- **Symptom:** macOS and Linux builds exist as pre-releases (DMG, `.deb`, `.rpm`, `.AppImage`) but are not yet fully QA'd — the MVP QA pass targets Windows.
- **Workaround:** Windows 10/11 is the primary supported platform; report macOS/Linux issues via GitHub Issues.
- **Fix planned:** 0.1.0 stable (cross-platform QA pass).

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
- **Status (beta.1):** `.ptz` format is **locked to v1** as of `v0.1.0-beta.1` (feature freeze). Backward compatibility is guaranteed from here: all subsequent format changes must be additive and ship with a migrator (`docs/guide/ptz-migration.md`). `v1.0.0` marks the final format lock.
- **Workaround:** Export to PNG/JPEG/WebP for long-term storage.

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
- **Fix planned:** 0.2.0 (if still observable after splashscreen removal).

### 14. Custom Titlebar Accessibility
- **Symptom:** Custom titlebar may not fully support keyboard navigation (Alt+Space system menu, F10 menu activation).
- **Mitigation (code evidence, 2026-08-02):** Native menu bar is installed via `app.set_menu` (`main.rs:125`) with `on_menu_event` routing to the frontend — Windows handles F10/Alt+Space activation for windows with a native menu bar. Workaround: standard keyboard shortcuts (Ctrl+N, Ctrl+O, etc.).
- **Status:** Pending manual verification on the v0.1.0 release build; fix only if F10/Alt+Space still fail.

### 15. CSP Allows `unsafe-inline` for Styles
- **Symptom:** Content Security Policy allows inline styles (required for Tailwind CSS v4).
- **Risk:** Low — does not affect scripts. CSS injection is limited risk.
- **Fix planned:** v1.0 (hash-based CSP).

## 📊 Performance Notes

### 16. Idle RAM: ~34 MB — well below 250 MB target.
### 17. Installer Size: 4-6 MB — well below 80 MB target.
### 18. Test Coverage: 2687 frontend (jsdom) + 84 Rust core cases.

## 🔄 Migration Path

### From v0.1.0 (first stable) onward
- `.ptz` backward-compat is guaranteed from `v0.1.0` (format locked to v1 in beta.1): all later format changes are additive and ship a migrator (`docs/guide/ptz-migration.md`). Files saved in v0.1.0 stay loadable.
- Settings and window state will migrate automatically.
- Breaking changes may still land during `0.x` MINOR bumps (per SemVer) — always documented in release notes.

## 📞 Reporting Issues

- **Bug reports:** https://github.com/photrez/photrez/issues
- **Security reports:** See `SECURITY.md` (report privately before public disclosure)
- **Feature requests:** Open a discussion or issue with `feature-request` label

Thank you for using Photrez! Your feedback helps shape the 0.2.0 and 1.0 releases.
