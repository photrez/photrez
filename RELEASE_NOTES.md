# Photrez v0.1.0-beta.1 — Feature-Freeze Release

This is the first beta of Photrez. It marks the feature freeze for the
0.1.0 MVP: the `.ptz` document format is now locked, and no new features
will be added until the stable release — only fixes.

## Release highlights

- **Feature freeze** — the MVP scope is locked; no new features until `0.1.0`, only fixes.
- **`.ptz` format locked to v1** — projects saved by beta releases stay readable by future stable versions.
- **Eraser fixed** — no more black edge artifacts at the end of an eraser stroke.
- **Ctrl+P fixed** — the app's print dialog opens; the browser's native print dialog no longer interferes.

## What changed

### New features

- None — this is a freeze release. All MVP features shipped in alpha releases are carried over.

### Improvements

- MVP scope frozen in `product-scope.md` (effective `0.1.0-beta.1`).
- `.ptz` v1 format locked; migration contract documented in `docs/guide/ptz-migration.md`.
- Command contract spec synced: `supported_commands` now reflects the public frontend subset.
- Beta.1 regression pass complete — 2685 frontend + 84 Rust core test cases green, type-check and build green.

### Bug fixes

- Ctrl+P no longer triggers the browser's native print dialog; the app's print dialog is used consistently.
- Eraser strokes no longer leave black outline artifacts in the final committed composite.

## Downloads

Installers are attached to this release (see the Assets section above):

- **Windows**: `.exe` (x64, NSIS)
- **macOS**: `.dmg` (Apple Silicon / ARM64)
- **Linux**: `.deb`, `.rpm`, `.AppImage` (x86_64)

## SHA-256 checksums

Photrez is not yet code-signed, so checksums are the only way to verify an
installer's integrity. The full list is attached as the `SHA256SUMS` asset;
spot-checking example:

```
$ sha256sum Photrez_0.1.0-beta.1_x64-setup.exe
3d112fe03315e253a6225e8b8fcfded1e664eff6151ab476d502716ef3d686f1  Photrez_0.1.0-beta.1_x64-setup.exe
```

Compare against the `SHA256SUMS` asset attached to this release.

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
