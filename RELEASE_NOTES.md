**2026-08-06 · Windows x64**

Photrez is out of beta. This first stable release ships the complete MVP: a full editing toolkit, the layered document model, and the locked `.ptz` project format.

## Download

| Platform | Download | Size |
| --- | --- | --- |
| Windows | [Photrez_0.1.0_x64-setup.exe](https://github.com/photrez/photrez/releases/download/v0.1.0/Photrez_0.1.0_x64-setup.exe) | ~6 MB |

Also available in the Assets section below. macOS and Linux builds will be produced from their native platforms and attached to future releases.

## What's inside

### Complete editing toolkit

- **Brush & Eraser** — pressure-style strokes with live cursor feedback; the eraser works on any layer without the black halo that plagued the beta.
- **Paint Bucket & Gradient** — flood-fill with tolerance and anti-aliased edges, plus linear and radial gradients.
- **Selection** — rectangular and elliptical selections with ratio and size constraints; move, transform, and resize selected content.
- **Crop & Resize** — canvas crop with a modern interaction mode, and exact document resize.
- **Eyedropper** — sample any color from the canvas into the foreground or background slot.

### Layers & document model

- **Full layer system** — add, delete, reorder, and rename layers; alpha-aware click-to-select; move and transform per layer.
- **Locked `.ptz` v1 format** — every project saved since `0.1.0-beta.1` opens in this release and will open in every future release.
- **Undo / Redo** — complete history across every tool and operation.

### Workflow

- **Print is roughly 4x faster** — A4 composite output drops from 139 MB to 35 MB by capping print DPI at 300, the photo-print standard. No visible quality loss at normal viewing distance.
- **Export & save** — export finished work, autosave, and native Windows save dialogs.
- **Drag & drop** — open files from the OS or another open project by dragging them in.
- **Desktop shell** — built on Tauri 2: native window chrome, dockable panels, and a fast WASM compositor.

## Improvements

- Print composite DPI capped at 300 — faster print jobs, identical output at normal viewing distance.
- Installers upgrade in place — running setup over an existing installation no longer asks to uninstall first.
- Test suite fully green with zero skipped tests: 2700 frontend + Rust core tests.

## Bug fixes

- Eraser no longer shows a black halo around the cursor during a live stroke.
- Print-to-PDF keeps working with the DPI cap (the save dialog flow is unchanged).

## Hardening

- `print_image_raw` validates the raw pixel buffer against its declared dimensions before handing it to the printer — prevents an out-of-bounds read if the two ever disagree.
- Print size constants now have a single source of truth — no more drift between worker, main thread, and preview.
- Paper-size fetch failures degrade gracefully to an empty list instead of surfacing an unhandled rejection.

## Verify the download

The installer is not yet code-signed, so the SHA-256 checksum is the way to confirm the file you downloaded is the one we built:

```text
b700c49f5353e076e77d4b1ae47f9571752462db5c0c80aee6afb157363e3eca  Photrez_0.1.0_x64-setup.exe
```

The full list is attached as the `SHA256SUMS` asset. Windows may show a Defender SmartScreen warning when launching — click **More info → Run anyway**.
