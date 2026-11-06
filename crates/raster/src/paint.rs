// SPDX-License-Identifier: AGPL-3.0-or-later
//! Paint rasterization surface.
//!
//! The paint candidate rasterizer is the reference implementation the host's
//! paint path is compared against. It depends on nothing but the standard
//! library, which is why this package can hold it and stay a leaf.

pub mod raster;

#[cfg(test)]
mod tests;
