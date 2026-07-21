# 36 - Glossary (Terminology Guide)

This document standardizes terminology across all Photrez documentation and implementation.

Use these exact terms consistently. Do not introduce synonyms without updating this file.

## 1) Product Terms

| Term | Definition | ❌ Do Not Use |
| --- | --- | --- |
| **Photrez** | The product name (working). | Legacy aliases: PS, PhotoEditor |
| **MVP** | Minimum Viable Product (version 1). | v1 beta, prototype, alpha |
| **Shell** | The Tauri desktop application layer. | Frontend, UI layer (ambiguous) |
| **Core** | The TypeScript DocumentEngine (current MVP). The Rust engine for document/editing logic (future target). | Backend, server |
| **Renderer** | The WebGL2-based rendering pipeline (current MVP). The wgpu-based rendering pipeline (future target). | Drawing engine, canvas engine |

## 2) Document Terms

| Term | Definition | ❌ Do Not Use |
| --- | --- | --- |
| **Document** | The root data structure containing canvas, layers, and history. | Project, file, workspace |
| **Canvas** | The pixel area defined by width × height where editing happens. | Artboard, board, page |
| **Canvas size** | The document's pixel dimensions (width × height). | Resolution (ambiguous without DPI context) |
| **Layer** | A single bitmap element in the layer stack. | Sheet, level, plane |
| **Background layer** | The bottom-most layer; cannot be deleted in MVP. | Base layer, default layer |
| **Layer stack** | The ordered list of layers from bottom to top. | Layer list (acceptable in UI), layer tree |

## 3) UI Region Terms

| Term | Definition | ❌ Do Not Use |
| --- | --- | --- |
| **Top Bar** | The horizontal bar at the top with file actions and title. | Header, navbar, menu bar |
| **Tool Rail** | The vertical toolbar on the left with tool icons. | Toolbox, sidebar (ambiguous) |
| **Viewport** | The central area where the canvas is displayed and manipulated. | Work area, canvas area (ambiguous with data model canvas) |
| **Inspector** | The right panel containing layers, properties, and history tabs. | Sidebar (ambiguous), right panel, properties panel |
| **Status Bar** | The bottom bar showing zoom, dimensions, and hints. | Footer, info bar |

## 4) Editing Terms

| Term | Definition | ❌ Do Not Use |
| --- | --- | --- |
| **Selection** | A pixel-level mask defining the active editing region. | Marquee (acceptable as tool name) |
| **Transform** | Scale, rotate, or flip operation on a layer or selection. | Modify, change |
| **Crop** | Reducing canvas size by defining a sub-region. | Trim, cut (ambiguous with clipboard) |
| **Resize** | Changing canvas or image dimensions with interpolation. | Scale (ambiguous with transform scale) |
| **Brush stroke** | A continuous drawing path from pointer-down to pointer-up. | Paint, draw (acceptable informally) |
| **Export** | Writing the final composited image to a file format. | Save as, render, output |
| **Save** | Writes the `.ptz` project file (all layers preserved) when the source is `.ptz`; overwrites a flat source in place for single-layer images. Export produces flat JPG/PNG/WebP. | — |

## 5) Technical Terms

| Term | Definition | ❌ Do Not Use |
| --- | --- | --- |
| **Command** | An IPC message from Shell to Core. | Request, action, event (ambiguous) |
| **Command contract** | The versioned schema for command payloads and responses. | API, protocol, interface |
| **Envelope** | The standard response wrapper (`{ ok, contract_version, data/error }`). | Response object, payload |
| **Error code** | The machine-readable error identifier (e.g., `E_VALIDATION`). | Error type, error name |
| **Bitmap** | Raw pixel data buffer (RGBA8 in MVP). | Raster data, pixel array, image data |
| **History entry** | A single undo/redo step stored in the history stack. | Snapshot, state, checkpoint |
| **Autosave** | Automatic background persistence for crash recovery. | Auto-backup, temp save |
| **Design token** | A named CSS custom property for colors, spacing, etc. | Variable, constant, theme value |

## 6) Bahasa Indonesia Term Mapping

For legacy bilingual documents (`00-id-summary.md`, originally `01-id-decision-log.md`, now `decision-log.md`):

| English | Bahasa Indonesia | Notes |
| --- | --- | --- |
| Document | Dokumen | |
| Layer | Layer | Keep English (industry standard) |
| Canvas | Canvas / Kanvas | Both acceptable |
| Viewport | Viewport | Keep English |
| Selection | Seleksi | |
| Transform | Transformasi | |
| Crop | Crop | Keep English |
| Export | Ekspor | |
| Import | Impor | |
| Brush | Brush / Kuas | Keep English for tool name |
| Eraser | Eraser / Penghapus | Keep English for tool name |
| Shortcut | Shortcut / Pintasan | Both acceptable |
| Undo | Undo | Keep English |
| Redo | Redo | Keep English |

## 7) Change Control

- New terms must be added here before use in docs or code.
- Term changes must be propagated to all affected documents.
- AI agents should reference this glossary when writing docs or code comments.
