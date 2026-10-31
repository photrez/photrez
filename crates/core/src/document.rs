// SPDX-License-Identifier: AGPL-3.0-or-later
// Runtime document bounded context. Owns the authoritative `DocumentCore`
// engine (state, command dispatch, history stream), the structural and canvas
// command families, the structural-sharing layer set, the document snapshot
// DTO, and the duplicate-name planner.
//
// `DocumentCore` and `DocumentSnapshot` are the items the rest of the crate
// consumes; every submodule below stays private unless a sibling crate-private
// module needs it. The imports below are the shared surface the child modules
// pick up through `use super::*;`.

use crate::pixel::state_node::StateNode;
use crate::pixel::tile::TilePatch;
use crate::ptz::content::{PtzDocumentContent, SelectionState};
use crate::ptz::projection::{project_known_layer, project_new_layer};
use crate::ptz::replica::PtzReplica;
use crate::render::*;
use crate::wire::command::*;
use crate::wire::history::*;
use crate::wire::projection::*;
use std::collections::HashSet;
use std::sync::Arc;

use self::history::*;
use self::layer_set::*;

pub use engine::DocumentCore;
pub use snapshot::{DocumentSnapshot, LayerSnapshot};

mod apply;
mod canvas;
pub(crate) mod crop_bake;
pub(crate) mod duplicate_name;
mod engine;
pub(crate) mod history;
pub(crate) mod layer_set;
pub(crate) mod rasterize_bake;
pub(crate) mod snapshot;
mod structural;

#[cfg(test)]
mod tests;
