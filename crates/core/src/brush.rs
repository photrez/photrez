// SPDX-License-Identifier: AGPL-3.0-or-later
//! Brush geometry: dab production and tip building.
//!
//! `brush::engine` is the stroke spacing/carry state machine that turns pointer
//! moves into dab positions; `brush::tip` builds the brush tip bitmap those
//! dabs are stamped with. Both are leaves: they depend on nothing but the
//! standard library.

pub mod engine;
pub mod tip;

#[cfg(test)]
mod tests;
