//! The canvas display seam: converts `photrez-core` render/pixel output into
//! GPUI paint calls. Owned by the render/display work; intentionally empty
//! until then.
//!
//! Must not: run on a non-UI thread, mutate the document, or be bypassed -
//! this module is the only sanctioned path from core pixels to the screen.
