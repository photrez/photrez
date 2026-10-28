// SPDX-License-Identifier: AGPL-3.0-or-later
//! Public typed protocol and wire facade. The former process-global protocol
//! registry is callerless and has been removed.
//!
//! ```compile_fail
//! use photrez_core::protocol::protocol_reset;
//! fn main() { protocol_reset("doc"); }
//! ```
// Re-export surface for the protocol engine. The implementations now live in
// the `document/` bounded context (engine, history, snapshot) and in `wire/`
// (command envelopes, projections). The frozen host consumes the DTO types;
// its legacy process-global engine registry is not part of this facade.

pub use crate::document::layer_set::LayerMeta;
pub use crate::document::DocumentCore;
pub use crate::render::layer::{Rect, RenderLayer, RenderLayerChange};
pub use crate::wire::command::*;
pub use crate::wire::history::{
    HistoryEntryView, HistoryQuery, Origin, PayloadKind, PendingExternalView,
};
pub use crate::wire::projection::{ProtocolError, RenderDelta, RenderSnapshot};
