# Photrez v0.1.0-beta.1 — <short title>

<2-3 sentences: what this release is about and who it is for. Example:
"This is the first beta of Photrez. It marks the feature freeze for the
0.1.0 MVP: the document format is now locked, and no new features will
be added until the stable release — only fixes.">

## Release highlights

- <One sentence per headline feature, focused on user impact.>
- <Example: "Layers panel with drag-and-drop reordering — organize your
  composition without hunting through menus.">

## What changed

### New features

- <Details, one bullet per item.>

### Improvements

- <Performance, UX, under-the-hood wins.>

### Bug fixes

- <Fixed issues, with issue numbers when known.>

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
<fill in hash>  Photrez_0.1.0-beta.1_x64-setup.exe
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
