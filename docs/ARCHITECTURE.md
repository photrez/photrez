# Photrez Architecture

Photrez is a native Rust desktop image editor built with GPUI.

## Runtime

- crates/photrez hosts the GPUI application, native window, input handling, and views.
- crates/core provides photrez-core, the canonical document, pixel, and history authority.
- crates/document provides photrez-document: the persisted layer content model, the
  layer parameters, and the layer metadata value the engine stores in history. It is a
  leaf and depends on no other package in this workspace.
- crates/raster provides photrez-raster: the canonical pixel buffer, the packed tile
  store, the copy-on-write state nodes, the compute kernels, and the paint
  rasterizer and brush geometry that prepare a stamp. It is a leaf and holds no
  document state.
- crates/format provides photrez-format: the `.ptz` envelope, its writer and reader,
  the seed-payload validation, and the image payload codecs. It depends on
  `photrez-document` only.
- crates/render provides photrez-render: the CPU compositor and the shape and text
  rasterizers. It reads pixels through `RenderInput` and names no registry.
- crates/xtask provides workspace tooling.

`photrez` depends on `photrez-core`. `photrez-core` depends on `photrez-document`,
`photrez-raster`, `photrez-format`, and `photrez-render`. `photrez-format` and
`photrez-render` depend on `photrez-document`, and `photrez-render` also depends on
`photrez-raster`; `photrez-document` and `photrez-raster` are leaves. The core
re-exports keep the application on one entry point, and
`scripts/check-package-boundaries.sh` enforces that graph.

The application has no embedded browser, JavaScript runtime, or application IPC bridge.

The crate-level ownership rules and invariants for `photrez-core` are in
[`crates/core/README.md`](../crates/core/README.md).

## State and rendering

Document state, pixels, and history belong to photrez-core. GPUI owns transient interface state. Views and GPU buffers are derived representations and must not become alternate document authorities.

Native window and input
        |
        v
GPUI application (crates/photrez)
        | document operations
        v
photrez-core (crates/core) ──┬─> photrez-document (crates/document)
                             ├─> photrez-raster   (crates/raster)
                             ├─> photrez-format   (crates/format)
                             └─> photrez-render   (crates/render)
        | derived document view
        v
Native display
