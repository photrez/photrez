# 33 - File Format Support (MVP)

This document defines which image formats Photrez can open (import) and export in MVP.

## 1) Import (Open) Formats

| Format | Extension(s) | Support Level | Decoder | Notes |
| --- | --- | --- | --- | --- |
| JPEG | `.jpg`, `.jpeg` | Full | `image` crate | Most common input format |
| PNG | `.png` | Full | `image` crate | Supports transparency |
| WebP | `.webp` | Full | `image` crate | Lossy and lossless |
| BMP | `.bmp` | Full | `image` crate | Legacy format, simple decoder |
| GIF | `.gif` | First frame only | `image` crate | Animated GIF opens first frame |

### Import Validation Rules

- Maximum import dimensions: `16384 x 16384` pixels.
- Maximum decoded size: `256 MB` (RGBA buffer), per `docs/spec/data-model.md`.
- If file exceeds limits: return `E_RESOURCE_LIMIT` with user-readable message.
- If file is malformed or unsupported: return `E_VALIDATION`.
- File extension must match actual format (basic magic byte check).
- All imported files are treated as untrusted input (per `docs/ARCHITECTURE.md` section 9).

### Import Behavior

1. User selects file via OS dialog (filtered to supported extensions).
2. Shell validates path and extension at boundary.
3. Core decodes image using `image` crate.
4. Decoded pixels are loaded into a new document with one raster layer.
5. Canvas size is set to image dimensions.
6. Default color profile (`sRGB`) is applied.

## 2) Export Formats

| Format | Extension | Quality Setting | Compression | Notes |
| --- | --- | --- | --- | --- |
| JPEG | `.jpg` | `0.0..1.0` (default `0.85`) | Lossy | No transparency support |
| PNG | `.png` | N/A (lossless) | Lossless | Supports transparency |
| WebP | `.webp` | `0.0..1.0` (default `0.85`) | Lossy | Supports transparency |
| TIFF | `.tiff` | n/a (lossless) | Lossless | Preserves alpha, large file |

### Export Validation Rules

- Quality parameter is normalized to `[0.0, 1.0]`.
- Quality is ignored for PNG (always lossless).
- Output dimensions match document dimensions unless explicit resize-on-export is used.
- Output path must be writable; `E_IO` returned if write fails.
- Export must not silently fail; deterministic success/error response required.
- If JPEG is selected and document has transparency: composite on white background first.

### Export Behavior

1. User triggers export (menu or `Ctrl+Shift+S`).
2. Format selection dialog appears with quality settings.
3. User selects output path.
4. Core flattens all visible layers into single bitmap.
5. Core encodes bitmap to selected format.
6. File is written to disk.
7. Success/failure feedback shown to user.

## 3) Non-Goals (MVP)

| Format | Reason |
| --- | --- |
| PSD | Out of MVP scope; planned for Layer B |
| TIFF | ✅ Exported in 0.2.0 (lossless, archival/print) |
| SVG | Vector format; Photrez is raster-only in MVP |
| RAW (CR2, NEF, ARW) | Requires specialized decoder; post-MVP |
| ICO | Niche format; not needed for target users |
| AVIF | Limited `image` crate support; evaluate post-MVP |
| HEIC/HEIF | Patent licensing complexity; evaluate post-MVP |

## 4) File Dialog Filter

The OS file dialog should filter to supported formats:

```
Image Files (*.jpg;*.jpeg;*.png;*.webp;*.bmp;*.gif)
JPEG (*.jpg;*.jpeg)
PNG (*.png)
WebP (*.webp)
BMP (*.bmp)
GIF (*.gif)
All Files (*.*)
```

## 5) MIME Type Mapping

| Extension | MIME Type |
| --- | --- |
| `.jpg` / `.jpeg` | `image/jpeg` |
| `.png` | `image/png` |
| `.webp` | `image/webp` |
| `.bmp` | `image/bmp` |
| `.gif` | `image/gif` |

## 6) Change Control

- Adding a new import format requires updating this file and `docs/reference/dependency-inventory.md`.
- Adding a new export format requires updating this file, `docs/spec/prd.md` (acceptance criteria), and `docs/reference/command-contract-spec.md` (export command).
