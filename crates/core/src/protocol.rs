// SPDX-License-Identifier: AGPL-3.0-or-later
// Re-export surface for the protocol engine. The implementations now live in
// document_core.rs / model.rs / history.rs / command.rs / projection.rs (the
// split of the former protocol.rs). This shim keeps `photrez_core::protocol::*`
// as the single public path to those types; the split modules are `pub(crate)`,
// so items not listed in the `pub use` lines below are crate-internal. No
// external crate currently consumes this path: the wasm-bindgen bridge exports
// flat JS function names (protocol_apply_command, etc.) and src-tauri does not
// reference `protocol::`.

pub use crate::command::*;
pub use crate::document_core::ProtocolEngine;
pub use crate::history::{Origin, PayloadKind};
pub use crate::model::{LayerMeta, Rect, RenderLayer, RenderLayerChange};
pub use crate::projection::{
    HistoryEntryView, HistoryQuery, PendingExternalView, ProtocolError, RenderDelta, RenderSnapshot,
};
