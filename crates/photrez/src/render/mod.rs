//! # render - the GPUI display seam
//!
//! [`canvas_bridge`] is THE single place where `photrez-core` pixel/render
//! output becomes GPUI paint calls. Nothing else in this crate is allowed to
//! paint the canvas.
//!
//! Must not own: document state or mutation (core), widget layout
//! (`views/`), or input policy (`input/`).

pub mod canvas_bridge;
