// SPDX-License-Identifier: AGPL-3.0-or-later
//! Public typed protocol and wire surface. The former process-global protocol
//! registry is callerless and has been removed.
//!
//! ```compile_fail
//! use photrez_core::protocol::protocol_reset;
//! fn main() { protocol_reset("doc"); }
//! ```
// Re-export surface for the protocol engine. The implementations live in the
// `engine` bounded context (state, history, snapshot) and in `wire` (command
// envelopes, projections). The `photrez` host consumes these DTO types; no
// process-global engine registry is part of this surface.
//
// The list is item by item, never a wildcard: `wire::command::*` was an
// unreviewed public surface, and an explicit list is what makes "every exported
// type has a caller or a written contract" checkable.

pub use crate::document::identity::ResourceId;
pub use crate::document::layer::{LayerChange, LayerMeta, Rect};
pub use crate::engine::DocumentCore;
pub use crate::wire::command::{
    BrushSettings, Command, CommandEnvelope, CommandResult, DocumentVersion, LockKind,
    PixelPatchHandoff, RasterizePixels, StrokePoint, TransformPatch, CONTRACT_VERSION,
};
pub use crate::wire::history::{
    HistoryEntryView, HistoryQuery, Origin, PayloadKind, PendingExternalView,
};
pub use crate::wire::projection::{ProtocolError, RenderDelta, RenderSnapshot};
