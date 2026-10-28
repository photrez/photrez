// SPDX-License-Identifier: AGPL-3.0-or-later
// The public CPU compositor facade plus the crate-private rasterizers and
// render-side DTOs. `compositor` is the single document-to-pixels entry point
// the native host may call; shape and text stay implementation details.

pub mod compositor;
pub(crate) mod layer;
pub(crate) mod shape;
pub(crate) mod text;

pub(crate) use layer::*;
