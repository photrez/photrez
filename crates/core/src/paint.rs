// SPDX-License-Identifier: AGPL-3.0-or-later
//! Paint parity facade.
//!
//! The paint candidate rasterizer is the reference implementation the host's
//! paint path is compared against. It is a leaf above `pixel`: it depends on
//! nothing but the standard library.

pub mod parity;

#[cfg(test)]
mod tests;
