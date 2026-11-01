# Photrez Architecture

Photrez is a native Rust desktop image editor built with GPUI.

## Runtime

- crates/photrez hosts the GPUI application, native window, input handling, and views.
- crates/core provides photrez-core, the canonical document, pixel, and history authority.
- crates/xtask provides workspace tooling.

The application has no embedded browser, JavaScript runtime, or application IPC bridge.

## State and rendering

Document state, pixels, and history belong to photrez-core. GPUI owns transient interface state. Views and GPU buffers are derived representations and must not become alternate document authorities.

Native window and input
        |
        v
GPUI application (crates/photrez)
        | document operations
        v
photrez-core (crates/core)
        | derived document view
        v
Native display
