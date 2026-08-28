// SPDX-License-Identifier: AGPL-3.0-or-later
// Ticket 1-2 — Protocol for Hybrid Command-Snapshot.
// No persistent TS mirror. Version checks are correctness.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

// C5.3-A: pixel-history payload references the canonical per-layer tile type.
use crate::pixel_store::TilePatch;

/// Schema/protocol version. Bump on breaking envelope change.
pub const CONTRACT_VERSION: u32 = 1;

pub type DocumentVersion = u64;
pub type ResourceId = u32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderLayer {
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub opacity: f64,
    pub resource_id: ResourceId,
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
    pub dirty_rect: Option<Rect>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum RenderLayerChange {
    Upsert { layer: RenderLayer },
    // ADR 0008 (DeleteLayer ticket): carries resourceId so a future Resource
    // Registry can drive lifecycle (release/retain) WITHOUT re-owning pixels.
    Remove { id: String, resource_id: ResourceId },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderSnapshot {
    pub version: DocumentVersion,
    pub layers: Vec<RenderLayer>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderDelta {
    pub base_version: DocumentVersion,
    pub version: DocumentVersion,
    pub changes: Vec<RenderLayerChange>,
}

impl RenderDelta {
    pub fn is_applicable(&self, rendered_version: DocumentVersion) -> bool {
        self.base_version == rendered_version
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransformPatch {
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StrokePoint {
    pub x: f64,
    pub y: f64,
    pub pressure: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrushSettings {
    pub size: f64,
    pub hardness: f64,
    pub opacity: f64,
    pub flow: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Command {
    Noop,
    Ping {
        echo: String,
    },
    AddLayer {
        name: String,
    },
    DeleteLayer {
        id: String,
    },
    TransformLayer {
        id: String,
        transform: TransformPatch,
    },
    SetOpacity {
        id: String,
        opacity: f64,
    },
    BrushStroke {
        layer_id: String,
        points: Vec<StrokePoint>,
        settings: BrushSettings,
    },
    Undo,
    Redo,
    // ADR 0008 H0: records a legacy TS transition into the canonical stream.
    // Advances DocumentVersion by exactly 1; payload stays behind the EXTERNAL
    // PayloadAdapter (token only) — never re-owned by Rust.
    RecordExternalTransition {
        label: String,
        affected_layer_ids: Vec<String>,
        adapter_id: String,
        token: String,
        memory_cost_bytes: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandEnvelope {
    pub contract_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<DocumentVersion>,
    pub command: Command,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    pub document_version: DocumentVersion,
    pub delta: RenderDelta,
    // ADR 0008 H0 walker handoff: "external" means the entry at the cursor is
    // owned by an external PayloadAdapter — the HOST executes it via its
    // adapter and then calls protocol_history_cursor_commit. Absent = applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_seq: Option<u64>,
}

// ── History stream (ADR 0008) ────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    Native,
    External { adapter_id: String },
}

#[derive(Debug, Clone)]
pub enum EntryPayload {
    // Built-in native adapter: before/after metadata snapshots (transitional
    // representation; inverse/patch forms come with later tickets).
    Native {
        before: Vec<RenderLayer>,
        after: Vec<RenderLayer>,
    },
    External {
        token: String,
    },
    /// C5.3-A: native pixel-history entry. Carries before/after tile deltas for
    /// one layer. The metadata-level `ProtocolEngine` is reused as the SINGLE
    /// authoritative history cursor; pixel deltas live here, not in `PixelLayer`
    /// (which keeps only the canonical buffer + epoch). Not serialized — the
    /// `HistoryEntryView` exposes only a `payload_ref` token.
    Pixel {
        layer_id: String,
        before: Vec<TilePatch>,
        after: Vec<TilePatch>,
    },
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub seq: u64,
    pub group_id: u64,
    pub origin: Origin,
    pub label: String,
    pub affected_layer_ids: Vec<String>,
    pub version_before: DocumentVersion,
    pub version_after: DocumentVersion,
    pub memory_cost_bytes: u64,
    pub payload: EntryPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntryView {
    pub seq: u64,
    pub group_id: u64,
    pub origin: String, // "native" | "external:<adapterId>"
    pub label: String,
    pub affected_layer_ids: Vec<String>,
    pub version_before: DocumentVersion,
    pub version_after: DocumentVersion,
    pub memory_cost_bytes: u64,
    pub payload_ref: Option<String>, // Some(token) for external entries
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQuery {
    pub cursor: u64,
    pub last_seq: u64,
    pub degraded_hint: bool, // always false engine-side; TS projection merges markers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_external: Option<PendingExternalView>,
    pub entries: Vec<HistoryEntryView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingExternalView {
    pub seq: u64,
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
}

// ── Engine ───────────────────────────────────────────────────────────────
// HISTORY BOUNDARY (Ticket 2): history/future store Vec<RenderLayer> METADATA snapshots only
// (id/name/resourceId/transform/opacity). They do NOT store pixel buffers and must NOT be
// interpreted as pixel history. ADR 0007 HistoryEntry {command, affectedResources, patch/inverse,
// memoryCost, transaction} is the target; this Vec is transitional and will be replaced
// before pixel ownership moves to Rust. Storing full pixel snapshots at 2048²/4K would be GBs.
#[derive(Debug)]
pub struct ProtocolEngine {
    version: DocumentVersion,
    next_resource: ResourceId,
    layers: Vec<RenderLayer>,
    // ADR 0008 H0: canonical stream. cursor = number of APPLIED entries
    // (0..=entries.len()). Independent from `version` (see ADR C1).
    entries: Vec<HistoryEntry>,
    cursor: usize,
    next_seq: u64,
    max_depth: usize, // C5.3-B: bound the pixel-only history stream (FIFO eviction of oldest entry).
    adapters: Vec<String>, // "native" is implicit and always available
    // External-pending barrier (H0 invariant): while Some, the host owes a
    // protocol_history_cursor_commit for (seq, direction). EVERY command
    // through apply() is rejected with E_EXTERNAL_PENDING until then — the
    // host must not stack a pending external transition with new work.
    pending_external: Option<(u64, String)>,
}

impl Default for ProtocolEngine {
    fn default() -> Self {
        Self {
            version: 0,
            next_resource: 1,
            layers: Vec::new(),
            entries: Vec::new(),
            cursor: 0,
            next_seq: 1,
            max_depth: 50,
            adapters: Vec::new(),
            pending_external: None,
        }
    }
}

impl ProtocolEngine {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn version(&self) -> DocumentVersion {
        self.version
    }
    pub fn snapshot(&self) -> RenderSnapshot {
        RenderSnapshot {
            version: self.version,
            layers: self.layers.clone(),
        }
    }

    // ── ADR 0008 H0 stream helpers ────────────────────────────────────────
    fn estimate_layers_bytes(layers: &[RenderLayer]) -> u64 {
        serde_json::to_string(layers)
            .map(|s| s.len() as u64)
            .unwrap_or(0)
    }

    // Opens a forward native entry at the cursor. Truncates the forward
    // region (invariant: new command truncates redo). Returns the entry index.
    fn begin_forward(&mut self, label: &str, affected: &[String]) -> usize {
        self.entries.truncate(self.cursor);
        let seq = self.next_seq;
        self.next_seq += 1;
        let entry = HistoryEntry {
            seq,
            group_id: seq, // H0: singleton groups only
            origin: Origin::Native,
            label: label.to_string(),
            affected_layer_ids: affected.to_vec(),
            version_before: self.version,
            version_after: self.version + 1,
            memory_cost_bytes: 0, // filled by finish_forward
            payload: EntryPayload::Native {
                before: self.layers.clone(),
                after: Vec::new(),
            },
        };
        self.entries.push(entry);
        self.entries.len() - 1
    }

    fn finish_forward(&mut self, idx: usize) {
        let cost_before = match self.entries.get(idx) {
            Some(e) => match &e.payload {
                EntryPayload::Native { before, .. } => Self::estimate_layers_bytes(before),
                _ => 0,
            },
            None => 0,
        };
        let cost_after = Self::estimate_layers_bytes(&self.layers);
        if let Some(e) = self.entries.get_mut(idx) {
            e.memory_cost_bytes = cost_before + cost_after;
            if let EntryPayload::Native { after, .. } = &mut e.payload {
                *after = self.layers.clone();
            }
        }
        self.cursor = self.entries.len();
    }

    // ── C5.3-A: native pixel history (unified cursor) ──────────────────────
    // These reuse the SAME `entries`/`cursor`/`next_seq`/`version` as the
    // metadata path, so the `ProtocolEngine` is the single authoritative
    // history for a document. Pixel deltas are stored inline; `PixelLayer`
    // keeps only the canonical buffer + epoch. The legacy per-layer
    // `undo_stack`/`redo_stack` were REMOVED in C5.3-B; the only pixel history
    // is now this stream, bounded to `max_depth`.
    pub fn apply_pixel_patch(
        &mut self,
        layer_id: &str,
        before: Vec<TilePatch>,
        after: Vec<TilePatch>,
    ) -> u64 {
        self.entries.truncate(self.cursor);
        let seq = self.next_seq;
        self.next_seq += 1;
        self.entries.push(HistoryEntry {
            seq,
            group_id: seq,
            origin: Origin::Native,
            label: "pixel".to_string(),
            affected_layer_ids: vec![layer_id.to_string()],
            version_before: self.version,
            version_after: self.version + 1,
            memory_cost_bytes: 0,
            payload: EntryPayload::Pixel {
                layer_id: layer_id.to_string(),
                before,
                after,
            },
        });
        self.cursor = self.entries.len();
        // C5.3-B eviction: bound the pixel-only stream to `max_depth` entries.
        // FIFO remove of the oldest entry; at the tip, cursor tracks `entries.len()`.
        if self.entries.len() > self.max_depth {
            self.entries.remove(0);
            self.cursor = self.entries.len();
        }
        self.version += 1;
        self.version
    }

    /// Undo the entry just below the cursor. For `Pixel` entries returns the
    /// affected layer id + `before` tiles (to replay onto `PixelLayer.pixels`).
    /// Non-pixel entries move the cursor (+1 version) but yield no tiles.
    /// Rejects while a `pending_external` barrier is set (E_EXTERNAL_PENDING).
    pub fn undo_pixel(
        &mut self,
    ) -> Result<(Option<String>, Option<Vec<TilePatch>>), ProtocolError> {
        if self.pending_external.is_some() {
            return Err(ProtocolError {
                code: "E_EXTERNAL_PENDING".to_string(),
                message: "external history transition pending; commit cursor first".to_string(),
            });
        }
        if self.cursor == 0 {
            return Ok((None, None));
        }
        let idx = self.cursor - 1;
        let entry = match self.entries.get(idx) {
            Some(e) => e,
            None => return Ok((None, None)),
        };
        let (layer, tiles) = match &entry.payload {
            EntryPayload::Pixel {
                layer_id, before, ..
            } => (Some(layer_id.clone()), Some(before.clone())),
            _ => (None, None),
        };
        self.cursor -= 1;
        self.version += 1;
        Ok((layer, tiles))
    }

    /// Redo the entry at the cursor. Symmetric to `undo_pixel` (returns `after`).
    pub fn redo_pixel(
        &mut self,
    ) -> Result<(Option<String>, Option<Vec<TilePatch>>), ProtocolError> {
        if self.pending_external.is_some() {
            return Err(ProtocolError {
                code: "E_EXTERNAL_PENDING".to_string(),
                message: "external history transition pending; commit cursor first".to_string(),
            });
        }
        if self.cursor >= self.entries.len() {
            return Ok((None, None));
        }
        let idx = self.cursor;
        let entry = match self.entries.get(idx) {
            Some(e) => e,
            None => return Ok((None, None)),
        };
        let (layer, tiles) = match &entry.payload {
            EntryPayload::Pixel {
                layer_id, after, ..
            } => (Some(layer_id.clone()), Some(after.clone())),
            _ => (None, None),
        };
        self.cursor += 1;
        self.version += 1;
        Ok((layer, tiles))
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor < self.entries.len()
    }

    pub fn register_adapter(&mut self, adapter_id: &str) {
        if adapter_id != "native" && !self.adapters.iter().any(|a| a == adapter_id) {
            self.adapters.push(adapter_id.to_string());
        }
    }

    pub fn record_external(
        &mut self,
        label: &str,
        affected: &[String],
        adapter_id: &str,
        token: &str,
        memory_cost_bytes: u64,
    ) -> Result<(), ProtocolError> {
        if adapter_id != "native" && !self.adapters.iter().any(|a| a == adapter_id) {
            return Err(ProtocolError {
                code: "E_UNKNOWN_ADAPTER".to_string(),
                message: format!("adapter '{}' is not registered", adapter_id),
            });
        }
        // New forward transition: truncate redo region, append, advance DV once.
        self.entries.truncate(self.cursor);
        let seq = self.next_seq;
        self.next_seq += 1;
        self.entries.push(HistoryEntry {
            seq,
            group_id: seq,
            origin: Origin::External {
                adapter_id: adapter_id.to_string(),
            },
            label: label.to_string(),
            affected_layer_ids: affected.to_vec(),
            version_before: self.version,
            version_after: self.version + 1,
            memory_cost_bytes,
            payload: EntryPayload::External {
                token: token.to_string(),
            },
        });
        self.cursor = self.entries.len();
        Ok(())
    }

    /// Host confirms an EXTERNAL step it executed via its adapter.
    /// undo: expects cursor == seq (entry just below cursor).
    /// redo: expects cursor == seq - 1 (entry just above cursor).
    /// Requires a matching pending_external barrier (set by the walker handoff).
    pub fn history_cursor_commit(
        &mut self,
        seq: u64,
        direction: &str,
    ) -> Result<CommandResult, ProtocolError> {
        let pending_matches = matches!(
            self.pending_external.as_ref(),
            Some((s, d)) if *s == seq && d == direction
        );
        let ok = match direction {
            "undo" => self.cursor == seq as usize,
            "redo" => self.cursor + 1 == seq as usize,
            _ => false,
        };
        if !ok || !pending_matches {
            return Err(ProtocolError {
                code: if self.pending_external.is_some() {
                    "E_CURSOR_MISMATCH".to_string()
                } else {
                    "E_CURSOR_MISMATCH".to_string()
                },
                message: format!(
                    "cursor {} incompatible with seq {} direction {} (pending: {:?})",
                    self.cursor, seq, direction, self.pending_external
                ),
            });
        }
        match direction {
            "undo" => self.cursor -= 1,
            _ => self.cursor += 1,
        }
        let dv = self.version + 1;
        self.version = dv;
        self.pending_external = None; // barrier cleared on success only
        Ok(CommandResult {
            document_version: dv,
            delta: RenderDelta {
                base_version: dv - 1,
                version: dv,
                changes: Vec::new(),
            },
            status: Some("external-confirmed".to_string()),
            external_seq: Some(seq),
        })
    }

    pub fn history_query(&self) -> HistoryQuery {
        let entries = self
            .entries
            .iter()
            .map(|e| HistoryEntryView {
                seq: e.seq,
                group_id: e.group_id,
                origin: match &e.origin {
                    Origin::Native => "native".to_string(),
                    Origin::External { adapter_id } => format!("external:{}", adapter_id),
                },
                label: e.label.clone(),
                affected_layer_ids: e.affected_layer_ids.clone(),
                version_before: e.version_before,
                version_after: e.version_after,
                memory_cost_bytes: e.memory_cost_bytes,
                payload_ref: match &e.payload {
                    EntryPayload::External { token } => Some(token.clone()),
                    EntryPayload::Native { .. } => None,
                    EntryPayload::Pixel { .. } => None,
                },
            })
            .collect();
        HistoryQuery {
            cursor: self.cursor as u64,
            last_seq: self.next_seq.saturating_sub(1),
            degraded_hint: false, // engine-side always false; TS merges markers
            pending_external: self
                .pending_external
                .as_ref()
                .map(|(s, d)| PendingExternalView {
                    seq: *s,
                    direction: d.clone(),
                }),
            entries,
        }
    }

    fn diff(old: &[RenderLayer], new: &[RenderLayer]) -> Vec<RenderLayerChange> {
        let mut changes = Vec::new();
        for l in new {
            match old.iter().find(|o| o.id == l.id) {
                Some(o) if o == l => {}
                _ => changes.push(RenderLayerChange::Upsert { layer: l.clone() }),
            }
        }
        for o in old {
            if !new.iter().any(|n| n.id == o.id) {
                changes.push(RenderLayerChange::Remove {
                    id: o.id.clone(),
                    resource_id: o.resource_id,
                });
            }
        }
        changes
    }

    pub fn apply(&mut self, envelope: CommandEnvelope) -> Result<CommandResult, ProtocolError> {
        if envelope.contract_version != CONTRACT_VERSION {
            return Err(ProtocolError {
                code: "E_CONTRACT_VERSION".to_string(),
                message: format!(
                    "expected contractVersion {} got {}",
                    CONTRACT_VERSION, envelope.contract_version
                ),
            });
        }
        if let Some(expected) = envelope.expected_version {
            if expected != self.version {
                return Err(ProtocolError {
                    code: "E_VERSION_MISMATCH".to_string(),
                    message: format!("expected version {} got {}", expected, self.version),
                });
            }
        }
        // External-pending barrier (ADR 0008 H0 invariant): a host handoff is
        // outstanding — no new command may enter until the cursor commit lands.
        if let Some((seq, dir)) = &self.pending_external {
            return Err(ProtocolError {
                code: "E_EXTERNAL_PENDING".to_string(),
                message: format!(
                    "external history transition pending: commit seq {} direction {} first",
                    seq, dir
                ),
            });
        }
        let base = self.version;
        // ADR 0008 H0: external record is its own transition event.
        if let Command::RecordExternalTransition {
            label,
            affected_layer_ids,
            adapter_id,
            token,
            memory_cost_bytes,
        } = envelope.command
        {
            self.record_external(
                &label,
                &affected_layer_ids,
                &adapter_id,
                &token,
                memory_cost_bytes,
            )?;
            self.version += 1;
            return Ok(CommandResult {
                document_version: self.version,
                delta: RenderDelta {
                    base_version: base,
                    version: self.version,
                    changes: Vec::new(),
                },
                status: Some("external-recorded".to_string()),
                external_seq: None,
            });
        }
        // Walker handoff marker: set when an undo/redo step lands on an
        // EXTERNAL entry — host executes via its adapter then confirms with
        // protocol_history_cursor_commit (which performs the DV bump).
        let mut external_handoff: Option<u64> = None;
        let mut handoff_dir = "";
        let changes = match envelope.command {
            Command::Noop => Vec::new(),
            Command::Ping { echo } => {
                let id = format!("ping:{}", echo);
                let layer = RenderLayer {
                    id: id.clone(),
                    name: echo.clone(),
                    visible: true,
                    opacity: 1.0,
                    resource_id: self.next_resource,
                    x: 0.0,
                    y: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 0.0,
                    dirty_rect: Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    }),
                };
                if let Some(pos) = self.layers.iter().position(|l| l.id == id) {
                    self.layers[pos] = layer.clone();
                } else {
                    self.next_resource += 1;
                    self.layers.push(layer.clone());
                }
                vec![RenderLayerChange::Upsert { layer }]
            }
            Command::AddLayer { name } => {
                let _e = self.begin_forward("Add Layer", &[]);
                let id = uuid::Uuid::new_v4().to_string();
                let layer = RenderLayer {
                    id: id.clone(),
                    name: name.clone(),
                    visible: true,
                    opacity: 1.0,
                    resource_id: self.next_resource,
                    x: 0.0,
                    y: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 0.0,
                    dirty_rect: Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    }),
                };
                self.next_resource += 1;
                self.layers.push(layer.clone());
                self.finish_forward(_e);
                vec![RenderLayerChange::Upsert { layer }]
            }
            Command::DeleteLayer { id } => {
                if let Some(pos) = self.layers.iter().position(|l| l.id == id) {
                    let _e = self.begin_forward("Delete Layer", &[id.clone()]);
                    let resource_id = self.layers[pos].resource_id;
                    self.layers.remove(pos);
                    self.finish_forward(_e);
                    vec![RenderLayerChange::Remove {
                        id: id.clone(),
                        resource_id,
                    }]
                } else {
                    Vec::new()
                }
            }
            Command::TransformLayer { id, transform } => {
                if let Some(pos) = self.layers.iter().position(|l| l.id == id) {
                    let _e = self.begin_forward("Transform Layer", &[id.clone()]);
                    let mut layer = self.layers[pos].clone();
                    layer.x = transform.x;
                    layer.y = transform.y;
                    layer.scale_x = transform.scale_x;
                    layer.scale_y = transform.scale_y;
                    layer.rotation = transform.rotation;
                    layer.dirty_rect = Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    });
                    self.layers[pos] = layer.clone();
                    self.finish_forward(_e);
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
                }
            }
            Command::SetOpacity { id, opacity } => {
                let clamped = opacity.clamp(0.0, 1.0);
                if let Some(pos) = self.layers.iter().position(|l| l.id == id) {
                    let _e = self.begin_forward("Set Opacity", &[id.clone()]);
                    let mut layer = self.layers[pos].clone();
                    layer.opacity = clamped;
                    layer.dirty_rect = Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    });
                    self.layers[pos] = layer.clone();
                    self.finish_forward(_e);
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
                }
            }
            Command::BrushStroke {
                layer_id,
                points,
                settings,
            } => {
                if points.is_empty() {
                    Vec::new()
                } else if let Some(pos) = self.layers.iter().position(|l| l.id == layer_id) {
                    let _e = self.begin_forward("Brush Stroke", &[layer_id.clone()]);
                    let mut layer = self.layers[pos].clone();
                    // dirtyRect must account for brush footprint, not just point bbox
                    let mut min_x = f64::INFINITY;
                    let mut max_x = f64::NEG_INFINITY;
                    let mut min_y = f64::INFINITY;
                    let mut max_y = f64::NEG_INFINITY;
                    for p in &points {
                        if p.x < min_x {
                            min_x = p.x;
                        }
                        if p.x > max_x {
                            max_x = p.x;
                        }
                        if p.y < min_y {
                            min_y = p.y;
                        }
                        if p.y > max_y {
                            max_y = p.y;
                        }
                    }
                    let radius = (settings.size / 2.0).ceil() as i32;
                    let x = (min_x.floor() as i32) - radius;
                    let y = (min_y.floor() as i32) - radius;
                    let w =
                        ((max_x - min_x).ceil() as i32 + settings.size.ceil() as i32).max(1) as u32;
                    let h =
                        ((max_y - min_y).ceil() as i32 + settings.size.ceil() as i32).max(1) as u32;
                    layer.dirty_rect = Some(Rect {
                        x,
                        y,
                        width: w,
                        height: h,
                    });
                    self.layers[pos] = layer.clone();
                    self.finish_forward(_e);
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
                }
            }
            // Handled by the early-return above (kept for exhaustiveness).
            Command::RecordExternalTransition { .. } => Vec::new(),
            Command::Undo => {
                if self.cursor == 0 {
                    // No-op undo: DV still bumps (accepted transition event).
                    Vec::new()
                } else {
                    let e = &self.entries[self.cursor - 1];
                    match &e.payload {
                        EntryPayload::Native { before, .. } => {
                            let new_layers = before.clone();
                            let changes = Self::diff(&self.layers, &new_layers);
                            self.layers = new_layers;
                            self.cursor -= 1;
                            changes
                        }
                        EntryPayload::Pixel { .. } => {
                            // Pixel undo is handled by undo_pixel/redo_pixel
                            // (the separate authoritative path); the metadata
                            // apply walker leaves pixel entries untouched.
                            Vec::new()
                        }
                        EntryPayload::External { .. } => {
                            // Host handoff: adapter executes, then
                            // protocol_history_cursor_commit moves the cursor
                            // and bumps DV. Nothing changes here.
                            external_handoff = Some(e.seq);
                            handoff_dir = "undo";
                            Vec::new()
                        }
                    }
                }
            }
            Command::Redo => {
                if self.cursor >= self.entries.len() {
                    Vec::new()
                } else {
                    let e = &self.entries[self.cursor];
                    match &e.payload {
                        EntryPayload::Native { after, .. } => {
                            let new_layers = after.clone();
                            let changes = Self::diff(&self.layers, &new_layers);
                            self.layers = new_layers;
                            self.cursor += 1;
                            changes
                        }
                        EntryPayload::Pixel { .. } => {
                            // See undo branch: pixel entries are owned by
                            // undo_pixel/redo_pixel, not the metadata walker.
                            Vec::new()
                        }
                        EntryPayload::External { .. } => {
                            external_handoff = Some(e.seq);
                            handoff_dir = "redo";
                            Vec::new()
                        }
                    }
                }
            }
        };
        if let Some(seq) = external_handoff {
            self.pending_external = Some((seq, handoff_dir.to_string()));
            return Ok(CommandResult {
                document_version: self.version,
                delta: RenderDelta {
                    base_version: base,
                    version: self.version,
                    changes: Vec::new(),
                },
                status: Some("external".to_string()),
                external_seq: Some(seq),
            });
        }
        self.version += 1;
        Ok(CommandResult {
            document_version: self.version,
            delta: RenderDelta {
                base_version: base,
                version: self.version,
                changes,
            },
            status: None,
            external_seq: None,
        })
    }
}

// ── wasm bridge — single shared ENGINE (module lifetime, survives location.reload() until WASM re-instantiated) ──
thread_local! {
    static ENGINE: std::cell::RefCell<ProtocolEngine> = std::cell::RefCell::new(ProtocolEngine::new());
}

#[wasm_bindgen]
pub fn protocol_contract_version() -> u32 {
    CONTRACT_VERSION
}

#[wasm_bindgen]
pub fn protocol_version() -> DocumentVersion {
    ENGINE.with(|cell| cell.borrow().version())
}

#[wasm_bindgen]
pub fn protocol_reset() {
    ENGINE.with(|cell| *cell.borrow_mut() = ProtocolEngine::new());
}

#[wasm_bindgen]
pub fn protocol_apply_command(envelope_json: &str) -> Result<String, JsValue> {
    let env: CommandEnvelope = serde_json::from_str(envelope_json)
        .map_err(|e| JsValue::from_str(&format!("E_ENVELOPE_PARSE: {}", e)))?;
    ENGINE.with(|cell| {
        let mut eng = cell.borrow_mut();
        eng.apply(env)
            .map(|r| serde_json::to_string(&r).unwrap())
            .map_err(|e| JsValue::from_str(&serde_json::to_string(&e).unwrap()))
    })
}

#[wasm_bindgen]
pub fn protocol_snapshot_json() -> String {
    ENGINE.with(|cell| serde_json::to_string(&cell.borrow().snapshot()).unwrap())
}

// ── ADR 0008 H0: history stream exports ─────────────────────────────────
#[wasm_bindgen]
pub fn protocol_register_payload_adapter(adapter_id: &str) {
    ENGINE.with(|cell| cell.borrow_mut().register_adapter(adapter_id));
}

#[wasm_bindgen]
pub fn protocol_history_query_json() -> String {
    ENGINE.with(|cell| serde_json::to_string(&cell.borrow().history_query()).unwrap())
}

#[wasm_bindgen]
pub fn protocol_history_cursor_commit(json: &str) -> Result<String, JsValue> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Req {
        seq: u64,
        direction: String,
    }
    let req: Req = serde_json::from_str(json)
        .map_err(|e| JsValue::from_str(&format!("E_ENVELOPE_PARSE: {}", e)))?;
    ENGINE.with(|cell| {
        cell.borrow_mut()
            .history_cursor_commit(req.seq, &req.direction)
            .map(|r| serde_json::to_string(&r).unwrap())
            .map_err(|e| JsValue::from_str(&serde_json::to_string(&e).unwrap()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_and_document_versions_are_distinct() {
        let mut eng = ProtocolEngine::new();
        assert_eq!(eng.version(), 0);
        let r = eng
            .apply(CommandEnvelope {
                contract_version: CONTRACT_VERSION,
                expected_version: None,
                command: Command::Noop,
            })
            .unwrap();
        assert_eq!(r.document_version, 1);
        assert_eq!(r.delta.base_version, 0);
        assert_eq!(r.delta.version, 1);
    }

    #[test]
    fn rejects_wrong_contract_version() {
        let mut eng = ProtocolEngine::new();
        let err = eng
            .apply(CommandEnvelope {
                contract_version: 999,
                expected_version: None,
                command: Command::Noop,
            })
            .unwrap_err();
        assert_eq!(err.code, "E_CONTRACT_VERSION");
    }

    #[test]
    fn snapshot_vs_delta_semantics() {
        let mut eng = ProtocolEngine::new();
        let r1 = eng
            .apply(CommandEnvelope {
                contract_version: CONTRACT_VERSION,
                expected_version: None,
                command: Command::Ping { echo: "a".into() },
            })
            .unwrap();
        assert!(r1.delta.is_applicable(0));
        assert!(!r1.delta.is_applicable(999));
        let snap = eng.snapshot();
        assert_eq!(snap.version, 1);
        assert_eq!(snap.layers.len(), 1);
    }

    #[test]
    fn add_delete_transform_opacity_single_owner() {
        let mut eng = ProtocolEngine::new();
        let a = eng
            .apply(CommandEnvelope {
                contract_version: CONTRACT_VERSION,
                expected_version: None,
                command: Command::AddLayer { name: "L1".into() },
            })
            .unwrap();
        assert_eq!(eng.snapshot().layers.len(), 1);
        let id = eng.snapshot().layers[0].id.clone();
        let res_id = eng.snapshot().layers[0].resource_id;
        let t = eng
            .apply(CommandEnvelope {
                contract_version: CONTRACT_VERSION,
                expected_version: None,
                command: Command::TransformLayer {
                    id: id.clone(),
                    transform: TransformPatch {
                        x: 10.0,
                        y: 5.0,
                        scale_x: 2.0,
                        scale_y: 2.0,
                        rotation: 15.0,
                    },
                },
            })
            .unwrap();
        assert_eq!(t.delta.base_version, a.delta.version);
        assert_eq!(eng.snapshot().layers[0].x, 10.0);
        assert_eq!(eng.snapshot().layers[0].resource_id, res_id); // stable
        let o = eng
            .apply(CommandEnvelope {
                contract_version: CONTRACT_VERSION,
                expected_version: None,
                command: Command::SetOpacity {
                    id: id.clone(),
                    opacity: 0.5,
                },
            })
            .unwrap();
        assert_eq!(eng.snapshot().layers[0].opacity, 0.5);
        assert!(o.delta.changes[0].clone().is_upsert());
        let d = eng
            .apply(CommandEnvelope {
                contract_version: CONTRACT_VERSION,
                expected_version: None,
                command: Command::DeleteLayer { id: id.clone() },
            })
            .unwrap();
        assert_eq!(eng.snapshot().layers.len(), 0);
        assert!(matches!(
            d.delta.changes[0],
            RenderLayerChange::Remove { .. }
        ));
    }

    trait IsUpsert {
        fn is_upsert(&self) -> bool;
    }
    impl IsUpsert for RenderLayerChange {
        fn is_upsert(&self) -> bool {
            matches!(self, RenderLayerChange::Upsert { .. })
        }
    }

    #[test]
    fn undo_redo_via_delta() {
        let mut eng = ProtocolEngine::new();
        eng.apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::AddLayer { name: "A".into() },
        })
        .unwrap();
        let id = eng.snapshot().layers[0].id.clone();
        eng.apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::SetOpacity {
                id: id.clone(),
                opacity: 0.3,
            },
        })
        .unwrap();
        assert_eq!(eng.snapshot().layers[0].opacity, 0.3);
        let u = eng
            .apply(CommandEnvelope {
                contract_version: CONTRACT_VERSION,
                expected_version: None,
                command: Command::Undo,
            })
            .unwrap();
        assert_eq!(eng.snapshot().layers[0].opacity, 1.0);
        assert!(u.delta.changes.iter().any(|c| c.is_upsert()));
        let r = eng
            .apply(CommandEnvelope {
                contract_version: CONTRACT_VERSION,
                expected_version: None,
                command: Command::Redo,
            })
            .unwrap();
        assert_eq!(eng.snapshot().layers[0].opacity, 0.3);
        assert!(r.delta.changes.iter().any(|c| c.is_upsert()));
    }

    #[test]
    fn brush_stroke_is_single_command() {
        let mut eng = ProtocolEngine::new();
        eng.apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::AddLayer { name: "L".into() },
        })
        .unwrap();
        let id = eng.snapshot().layers[0].id.clone();
        let v_before = eng.version();
        let s = eng
            .apply(CommandEnvelope {
                contract_version: CONTRACT_VERSION,
                expected_version: None,
                command: Command::BrushStroke {
                    layer_id: id.clone(),
                    points: vec![
                        StrokePoint {
                            x: 0.0,
                            y: 0.0,
                            pressure: 0.5,
                        },
                        StrokePoint {
                            x: 10.0,
                            y: 10.0,
                            pressure: 0.8,
                        },
                    ],
                    settings: BrushSettings {
                        size: 20.0,
                        hardness: 0.5,
                        opacity: 1.0,
                        flow: 1.0,
                    },
                },
            })
            .unwrap();
        assert_eq!(s.delta.base_version, v_before);
        assert_eq!(eng.version(), v_before + 1);
        assert_eq!(s.delta.changes.len(), 1);
        // dirtyRect must include footprint, not just point bbox
        if let RenderLayerChange::Upsert { layer } = &s.delta.changes[0] {
            // points 0,0 to 10,10 with size 20 => width/height at least 30
            assert!(layer.dirty_rect.as_ref().unwrap().width >= 30);
        } else {
            panic!("expected upsert");
        }
    }

    #[test]
    fn expected_version_matches_accepted() {
        let mut eng = ProtocolEngine::new();
        eng.apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::AddLayer { name: "A".into() },
        })
        .unwrap();
        let v = eng.version();
        let r = eng
            .apply(CommandEnvelope {
                contract_version: CONTRACT_VERSION,
                expected_version: Some(v),
                command: Command::AddLayer { name: "B".into() },
            })
            .unwrap();
        assert_eq!(r.delta.base_version, v);
        assert_eq!(eng.snapshot().layers.len(), 2);
    }

    #[test]
    fn expected_version_stale_rejected_document_unchanged() {
        let mut eng = ProtocolEngine::new();
        eng.apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::AddLayer { name: "A".into() },
        })
        .unwrap();
        let v = eng.version();
        let before_layers = eng.snapshot().layers.clone();
        let err = eng
            .apply(CommandEnvelope {
                contract_version: CONTRACT_VERSION,
                expected_version: Some(999),
                command: Command::AddLayer {
                    name: "stale".into(),
                },
            })
            .unwrap_err();
        assert_eq!(err.code, "E_VERSION_MISMATCH");
        assert_eq!(eng.version(), v);
        assert_eq!(eng.snapshot().layers, before_layers);
    }

    #[test]
    fn two_concurrent_same_expected_version_exactly_one_accepted() {
        let mut eng = ProtocolEngine::new();
        eng.apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::AddLayer { name: "A".into() },
        })
        .unwrap();
        let v = eng.version();
        // two concurrent commands with same expected v
        let r1 = eng.apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: Some(v),
            command: Command::AddLayer { name: "B".into() },
        });
        assert!(r1.is_ok());
        let r2 = eng.apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: Some(v),
            command: Command::AddLayer { name: "C".into() },
        });
        assert!(r2.is_err());
        assert_eq!(r2.unwrap_err().code, "E_VERSION_MISMATCH");
        // only one of B/C added
        assert_eq!(eng.snapshot().layers.len(), 2);
    }

    #[test]
    fn retry_after_snapshot_succeeds_against_new_version() {
        let mut eng = ProtocolEngine::new();
        eng.apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::AddLayer { name: "A".into() },
        })
        .unwrap();
        let v0 = eng.version();
        // stale attempt
        let err = eng
            .apply(CommandEnvelope {
                contract_version: CONTRACT_VERSION,
                expected_version: Some(999),
                command: Command::AddLayer {
                    name: "stale".into(),
                },
            })
            .unwrap_err();
        assert_eq!(err.code, "E_VERSION_MISMATCH");
        // retry with correct version (snapshot version)
        let r = eng
            .apply(CommandEnvelope {
                contract_version: CONTRACT_VERSION,
                expected_version: Some(v0),
                command: Command::AddLayer { name: "B".into() },
            })
            .unwrap();
        assert_eq!(r.delta.base_version, v0);
        assert_eq!(eng.snapshot().layers.len(), 2);
    }
}

// ── ADR 0008 H0 stream tests ─────────────────────────────────────────────
#[cfg(test)]
mod h0_tests {
    use super::*;

    fn env(cmd: Command) -> CommandEnvelope {
        CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: cmd,
        }
    }
    fn env_ev(cmd: Command, ev: u64) -> CommandEnvelope {
        CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: Some(ev),
            command: cmd,
        }
    }

    #[test]
    fn native_append_records_seq_and_cursor_advances() {
        let mut eng = ProtocolEngine::new();
        eng.apply(env(Command::AddLayer { name: "A".into() }))
            .unwrap();
        eng.apply(env(Command::AddLayer { name: "B".into() }))
            .unwrap();
        let q = eng.history_query();
        assert_eq!(q.cursor, 2);
        assert_eq!(q.last_seq, 2);
        assert_eq!(q.entries.len(), 2);
        assert_eq!(q.entries[0].seq, 1);
        assert_eq!(q.entries[1].seq, 2); // seq uniqueness/monotonicity
        assert_eq!(q.entries[0].origin, "native");
    }

    #[test]
    fn cursor_walks_back_and_forward_with_native_payload() {
        let mut eng = ProtocolEngine::new();
        eng.apply(env(Command::AddLayer { name: "A".into() }))
            .unwrap();
        let dv_after_add = eng.version();
        let r = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(r.status, None);
        assert_eq!(eng.history_query().cursor, 0);
        assert_eq!(eng.version(), dv_after_add + 1); // undo is itself a transition
        let r2 = eng.apply(env(Command::Redo)).unwrap();
        assert_eq!(r2.status, None);
        assert_eq!(eng.history_query().cursor, 1);
        assert!(r2
            .delta
            .changes
            .iter()
            .any(|c| matches!(c, RenderLayerChange::Upsert { .. })));
    }

    #[test]
    fn document_version_independent_from_cursor_on_noop_undo() {
        let mut eng = ProtocolEngine::new();
        let v0 = eng.version(); // cursor already 0
        let r = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(eng.history_query().cursor, 0); // cursor unchanged
        assert_eq!(r.document_version, v0 + 1); // DV advanced anyway
        assert!(r.delta.changes.is_empty());
    }

    #[test]
    fn redo_region_truncated_by_new_forward_command() {
        let mut eng = ProtocolEngine::new();
        eng.apply(env(Command::AddLayer { name: "A".into() }))
            .unwrap();
        eng.apply(env(Command::Undo)).unwrap(); // cursor=0
        eng.apply(env(Command::AddLayer { name: "B".into() }))
            .unwrap(); // truncates forward
        let q = eng.history_query();
        assert_eq!(q.cursor, 1);
        assert_eq!(q.entries.len(), 1);
        assert_eq!(q.entries[0].label, "Add Layer");
        assert_eq!(q.last_seq, 2); // seq NOT reused (next_seq monotonic)
        let r = eng.apply(env(Command::Redo)).unwrap();
        assert!(r.delta.changes.is_empty()); // nothing to redo
    }

    #[test]
    fn external_record_requires_registered_adapter_and_advances_dv_once() {
        let mut eng = ProtocolEngine::new();
        let err = eng
            .apply(env(Command::RecordExternalTransition {
                label: "L".into(),
                affected_layer_ids: vec![],
                adapter_id: "ts-external".into(),
                token: "t1".into(),
                memory_cost_bytes: 10,
            }))
            .unwrap_err();
        assert_eq!(err.code, "E_UNKNOWN_ADAPTER");

        eng.register_adapter("ts-external");
        let v0 = eng.version();
        let r = eng
            .apply(env(Command::RecordExternalTransition {
                label: "Move Layer".into(),
                affected_layer_ids: vec!["bg".into()],
                adapter_id: "ts-external".into(),
                token: "tok-1".into(),
                memory_cost_bytes: 128,
            }))
            .unwrap();
        assert_eq!(r.document_version, v0 + 1); // exactly once
        assert_eq!(r.status.as_deref(), Some("external-recorded"));
        let q = eng.history_query();
        assert_eq!(q.cursor, 1);
        assert_eq!(q.entries[0].origin, "external:ts-external");
        assert_eq!(q.entries[0].payload_ref.as_deref(), Some("tok-1"));
    }

    #[test]
    fn external_handoff_status_then_cursor_commit_bumps_dv_once() {
        let mut eng = ProtocolEngine::new();
        eng.register_adapter("ts-external");
        eng.apply(env(Command::RecordExternalTransition {
            label: "legacy op".into(),
            affected_layer_ids: vec![],
            adapter_id: "ts-external".into(),
            token: "t".into(),
            memory_cost_bytes: 1,
        }))
        .unwrap();
        let dv_before = eng.version();
        let r = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(r.status.as_deref(), Some("external"));
        assert_eq!(r.external_seq, Some(1));
        assert_eq!(r.document_version, dv_before); // NO bump at handoff time
        assert_eq!(eng.history_query().cursor, 1); // cursor untouched yet

        let c = eng.history_cursor_commit(1, "undo").unwrap();
        assert_eq!(c.document_version, dv_before + 1); // exactly one bump
        assert_eq!(eng.history_query().cursor, 0);

        // mismatched commit rejected
        let err = eng.history_cursor_commit(5, "undo").unwrap_err();
        assert_eq!(err.code, "E_CURSOR_MISMATCH");
    }

    #[test]
    fn external_pending_barrier_blocks_forward_and_history_commands() {
        let mut eng = ProtocolEngine::new();
        eng.register_adapter("ts-external");
        eng.apply(env(Command::RecordExternalTransition {
            label: "legacy op".into(),
            affected_layer_ids: vec![],
            adapter_id: "ts-external".into(),
            token: "t".into(),
            memory_cost_bytes: 1,
        }))
        .unwrap();
        let hand = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(hand.status.as_deref(), Some("external"));
        assert!(eng.pending_external.is_some());

        // mutating command during pending -> E_EXTERNAL_PENDING
        let e1 = eng
            .apply(env(Command::AddLayer { name: "X".into() }))
            .unwrap_err();
        assert_eq!(e1.code, "E_EXTERNAL_PENDING");

        // another history command during pending -> E_EXTERNAL_PENDING
        let e2 = eng.apply(env(Command::Undo)).unwrap_err();
        assert_eq!(e2.code, "E_EXTERNAL_PENDING");

        // mismatched commit during pending -> E_CURSOR_MISMATCH, barrier retained
        let e3 = eng.history_cursor_commit(99, "undo").unwrap_err();
        assert_eq!(e3.code, "E_CURSOR_MISMATCH");
        assert!(eng.pending_external.is_some());

        // correct commit clears the barrier; forward command now accepted
        eng.history_cursor_commit(1, "undo").unwrap();
        assert!(eng.pending_external.is_none());
        eng.apply(env(Command::AddLayer { name: "Y".into() }))
            .unwrap(); // must not throw
    }

    #[test]
    fn query_exposes_pending_external_state() {
        let mut eng = ProtocolEngine::new();
        eng.register_adapter("ts-external");
        // no pending initially
        assert!(eng.history_query().pending_external.is_none());
        // record + undo creates a real handoff -> pending exposed via query
        eng.apply(env(Command::RecordExternalTransition {
            label: "L2".into(),
            affected_layer_ids: vec![],
            adapter_id: "ts-external".into(),
            token: "t2".into(),
            memory_cost_bytes: 0,
        }))
        .unwrap();
        let h = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(h.status.as_deref(), Some("external"));
        let pend = eng
            .history_query()
            .pending_external
            .expect("pending exposed in query");
        assert_eq!(pend.seq, 1);
        assert_eq!(pend.direction, "undo");
    }

    #[test]
    fn cursor_commit_without_pending_rejected() {
        let mut eng = ProtocolEngine::new();
        let err = eng.history_cursor_commit(1, "undo").unwrap_err();
        assert_eq!(err.code, "E_CURSOR_MISMATCH");
    }
    #[test]
    fn stale_expected_version_still_rejected_on_stream_path() {
        let mut eng = ProtocolEngine::new();
        eng.apply(env(Command::AddLayer { name: "A".into() }))
            .unwrap();
        let err = eng
            .apply(env_ev(
                Command::TransformLayer {
                    id: "x".into(),
                    transform: TransformPatch {
                        x: 1.0,
                        y: 0.0,
                        scale_x: 1.0,
                        scale_y: 1.0,
                        rotation: 0.0,
                    },
                },
                999,
            ))
            .unwrap_err();
        assert_eq!(err.code, "E_VERSION_MISMATCH");
    }

    #[test]
    fn pixel_undo_redo_rejected_while_external_pending() {
        let mut eng = ProtocolEngine::new();
        eng.register_adapter("ts-external");
        eng.apply(env(Command::RecordExternalTransition {
            label: "legacy op".into(),
            affected_layer_ids: vec![],
            adapter_id: "ts-external".into(),
            token: "t".into(),
            memory_cost_bytes: 1,
        }))
        .unwrap();
        let hand = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(hand.status.as_deref(), Some("external"));
        assert!(eng.pending_external.is_some());
        // The SAME barrier must block the unified pixel history path.
        let u = eng.undo_pixel();
        assert!(u.is_err(), "pixel undo must reject while external pending");
        assert_eq!(u.unwrap_err().code, "E_EXTERNAL_PENDING");
        let r = eng.redo_pixel();
        assert!(r.is_err(), "pixel redo must reject while external pending");
        assert_eq!(r.unwrap_err().code, "E_EXTERNAL_PENDING");
    }

    // C5.3-B eviction: the pixel-only stream is bounded to `max_depth` (50)
    // entries. FIFO eviction of the oldest entry; cursor tracks the retained
    // stream; seq/version stay monotonic; canonical pixels are never touched.
    #[test]
    fn pixel_history_eviction_bounds_at_50() {
        let tp = |v: u8| TilePatch {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            data: vec![v; 4],
        };
        let mut e = ProtocolEngine::new();

        // case 1: single entry
        e.apply_pixel_patch("L", vec![tp(0)], vec![tp(1)]);
        assert_eq!(e.entries.len(), 1);
        assert_eq!(e.cursor(), 1);
        assert_eq!(e.version(), 1);

        // case 2: 50 entries, all retained
        for i in 2..=50 {
            e.apply_pixel_patch("L", vec![tp(i - 1)], vec![tp(i)]);
        }
        assert_eq!(e.entries.len(), 50);
        assert_eq!(e.cursor(), 50);
        assert_eq!(e.version(), 50);
        let seqs: Vec<u64> = e.entries.iter().map(|x| x.seq).collect();
        assert!(seqs.windows(2).all(|w| w[0] < w[1]), "seq monotonic");

        // case 3: 51st entry evicts the oldest (seq 1); stream stays bounded
        e.apply_pixel_patch("L", vec![tp(50)], vec![tp(51)]);
        assert_eq!(e.entries.len(), 50, "bounded at 50");
        assert_eq!(e.cursor(), 50, "cursor tracks retained stream");
        assert_eq!(e.version(), 51, "version monotonic");
        assert_eq!(e.entries[0].seq, 2, "oldest entry (seq 1) evicted");
        // retained payloads are intact (not corrupted by eviction)
        match &e.entries[49].payload {
            EntryPayload::Pixel { after, .. } => assert_eq!(after[0].data[0], 51),
            _ => panic!("expected Pixel payload"),
        }

        // case 4: undo after eviction
        let (layer, tiles) = e.undo_pixel().unwrap();
        assert_eq!(layer, Some("L".to_string()));
        assert_eq!(tiles.unwrap()[0].data[0], 50);
        assert_eq!(e.cursor(), 49);
        assert_eq!(e.version(), 52);

        // case 5: redo after eviction
        let (_, tiles) = e.redo_pixel().unwrap();
        assert_eq!(tiles.unwrap()[0].data[0], 51);
        assert_eq!(e.cursor(), 50);

        // case 6: undo to the eviction boundary, then a new entry truncates redo
        while e.can_undo() {
            e.undo_pixel().unwrap();
        }
        assert_eq!(e.cursor(), 0);
        assert!(
            e.undo_pixel().unwrap() == (None, None),
            "cannot undo past eviction boundary"
        );
        e.apply_pixel_patch("L", vec![tp(1)], vec![tp(99)]);
        assert_eq!(e.cursor(), 1);
        assert!(e.entries.len() <= 50, "still bounded");
    }
}
