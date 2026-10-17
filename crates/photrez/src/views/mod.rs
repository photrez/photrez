//! # views - screen-level regions
//!
//! Owns: composition of screen regions (title bar, tool rail, document tabs,
//! option bar, docks, status bar, canvas area) and the `Entity` state they
//! hold, handed down from `app/`.
//!
//! Must not own: reusable visual vocabulary (that is `components/`), canvas
//! painting (`render/`), or document truth (`photrez-core`).
