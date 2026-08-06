# Photrez v0.1.0 — First Stable Release

Photrez is out of beta. The MVP scope is locked at `0.1.0-beta.1` and this
stable release ships the fixes from the beta cycle plus a new MSI installer
for Windows. No new features were added in this release.

## Release highlights

- **First stable release** — the `.ptz` v1 format is locked; projects saved
  since `0.1.0-beta.1` stay readable in this and future releases.
- **Print is fast** — composite DPI is capped at 300, the industry standard
  for photo print. Printing to "Microsoft Print to PDF" is roughly 4x faster
  on A4 (was 4961×7016 px ≈ 139 MB, now 2480×3508 px ≈ 35 MB) with no visible
  loss; GDI scales up to the printer DPI cheaply.
- **Eraser clean** — the black halo that appeared around the eraser while
  dragging is gone. Position feedback now comes only from the cursor ring,
  never from the committed layer.
- **New: Windows MSI installer** — alongside the NSIS `.exe`. Both installers
  are unsigned; verify with the SHA-256 checksums below.
- **Hardened IPC** — the print command now validates the raw pixel buffer
  against its declared dimensions before handing it to GDI, preventing an
  out-of-bounds read if the two ever disagree.

## What changed

### Improvements

- Print composite DPI capped at 300 — faster print jobs, identical output at
  normal viewing distance.
- Test suite fully green with zero skipped tests: 2700 frontend + Rust core
  tests.

### Bug fixes

- Eraser no longer shows a black halo around the cursor during a live stroke.
- Print-to-PDF keeps working with the DPI cap (the save dialog flow is
  unchanged).

### Hardening

- `print_image_raw` validates `data length == width × height × 4` before
  rendering (IPC trust-boundary guard, covered by unit tests).
- Print size constants (`TARGET_PRINT_DPI`, `MAX_PX`) now have a single
  source of truth — no more drift between worker, main thread, and preview.

## Downloads

Installers are attached to this release (see the Assets section above):

- **Windows**: `.exe` (x64, NSIS) and `.msi` (x64, MSI)
- **macOS**: `.dmg` (Apple Silicon / ARM64)
- **Linux**: `.deb`, `.rpm`, `.AppImage` (x86_64)

## SHA-256 checksums

Photrez is not yet code-signed, so checksums are the only way to verify an
installer's integrity. The full list is attached as the `SHA256SUMS` asset;
spot-checking example:

```
$ sha256sum Photrez_0.1.0_x64-setup.exe
<empty>  Photrez_0.1.0_x64-setup.exe
```

Compare against the `SHA256SUMS` asset attached to this release. (Checksum
values are filled in after the installer is built.)

## Running on Windows and macOS

Photrez is not yet code-signed, so Windows and macOS may show warnings when
launching the app.

- **Windows**: You may see a Windows Defender SmartScreen warning.
  Click **"More info" -> "Run anyway"** to proceed.
- **macOS**: You'll need to remove the quarantine flag after installation,
  otherwise macOS may report the app as corrupted:

```bash
xattr -dr com.apple.quarantine /Applications/Photrez.app
```