//! # assets - compiled-in binary resources
//!
//! Owns: SVG icon data (verbatim from `lucide-solid@1.23.0`) consumed by
//! gpui-kit's SVG renderer, and the Inter font bytes referenced as a const in
//! `main.rs`.
//!
//! Must not own: nothing else lands here until a consuming module needs it.
//! Each leaf (e.g. [`icons`]) carries its own `#![allow(...)]` for the
//! pre-consumer window.

pub mod icons;
