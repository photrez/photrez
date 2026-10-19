//! Mounted navigation regression tests for the locked surfaces (L1-L11).
//!
//! Mounts the real view tree in a headless gpui-kit test window and asserts the
//! navigation contract: the region set, the dock's two-column layout with its
//! fixed navigator, and tab / panel ownership. Breaking any locked surface must
//! redden a test here; the RED transcripts live with each test.
//!
//! Needs `gpui-kit/test-support` (enabled from `[dev-dependencies]`).
