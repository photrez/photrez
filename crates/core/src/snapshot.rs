// SPDX-License-Identifier: AGPL-3.0-or-later
//! Rust-side atomic document snapshot DTO for undo/redo.
//!
//! A snapshot is a METADATA + bitmap-token transport, NOT a pixel store:
//!   - It carries per-layer pixel metadata (width/height/epoch/pixel_version)
//!     plus an OPAQUE `bitmap_token` that references the ImageBitmap TS owns.
//!   - Rust NEVER stores an ImageBitmap or pixel bytes here; the token lets the
//!     TS adapter restore the correct ImageBitmap after an undo/redo step, so a
//!     single `EntryPayload::Snapshot` entry restores BOTH metadata (this DTO)
//!     and pixel state (via the token reference) atomically.
//!
//! This is a RUST-CORE capability introduced with the migration flag OFF: the
//! row-major `PixelLayer` + TS `CommandHistory` remain the ACTIVE default. The
//! full document metadata (visible/opacity/blend/transform/selection/viewport)
//! is a later increment; this carries the pixel-layer metadata + token only.

use serde::{Deserialize, Serialize};

/// One layer's pixel metadata + opaque bitmap reference in a document snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LayerSnapshot {
    pub layer_id: String,
    pub width: u32,
    pub height: u32,
    /// Opaque reference to the ImageBitmap TS owns. `None` when the layer has no
    /// pixel store yet (e.g. an empty layer). Rust never dereferences the token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitmap_token: Option<String>,
    /// Layer mutation counter (mirrors the canonical store's epoch).
    pub epoch: u64,
    /// Pixel-mutation counter (mirrors the layer's `epoch`; the Rust store has no
    /// separate pixel_version field yet — kept as a distinct field for the later
    /// metadata increment).
    pub pixel_version: u64,
}

impl LayerSnapshot {
    /// Construct a layer snapshot with no bitmap token and zero counters.
    pub fn new(layer_id: &str, width: u32, height: u32) -> Self {
        Self {
            layer_id: layer_id.to_string(),
            width,
            height,
            bitmap_token: None,
            epoch: 0,
            pixel_version: 0,
        }
    }

    /// Builder-style attach of an opaque bitmap token (the pixel reference).
    pub fn with_bitmap_token(mut self, token: impl Into<String>) -> Self {
        self.bitmap_token = Some(token.into());
        self
    }
}

/// A document's atomic metadata + bitmap-token snapshot for undo/redo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSnapshot {
    pub doc_id: String,
    pub version: u64,
    pub layers: Vec<LayerSnapshot>,
}

impl DocumentSnapshot {
    /// Construct an empty snapshot for `doc_id` at `version`.
    pub fn new(doc_id: &str, version: u64) -> Self {
        Self {
            doc_id: doc_id.to_string(),
            version,
            layers: Vec::new(),
        }
    }

    /// Builder-style append of a layer snapshot.
    pub fn with_layer(mut self, layer: LayerSnapshot) -> Self {
        self.layers.push(layer);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pixel_store::{PixelStoreRegistry, TilePatch};

    #[test]
    fn layer_snapshot_construction_defaults() {
        let l = LayerSnapshot::new("L1", 512, 512);
        assert_eq!(l.layer_id, "L1");
        assert_eq!(l.width, 512);
        assert_eq!(l.height, 512);
        assert_eq!(l.bitmap_token, None);
        assert_eq!(l.epoch, 0);
        assert_eq!(l.pixel_version, 0);
    }

    #[test]
    fn document_snapshot_builders_append_layers() {
        let s = DocumentSnapshot::new("docA", 3)
            .with_layer(LayerSnapshot::new("L1", 16, 16))
            .with_layer(LayerSnapshot::new("L2", 32, 32).with_bitmap_token("tok-2"));
        assert_eq!(s.doc_id, "docA");
        assert_eq!(s.version, 3);
        assert_eq!(s.layers.len(), 2);
        assert_eq!(s.layers[1].bitmap_token.as_deref(), Some("tok-2"));
    }

    #[test]
    fn snapshot_roundtrip_serialization_byte_stable() {
        let s = DocumentSnapshot::new("docA", 5)
            .with_layer(LayerSnapshot::new("L1", 64, 64).with_bitmap_token("tok-1"))
            .with_layer(LayerSnapshot::new("L2", 128, 128));
        // byte-stable: serialize then deserialize yields the exact same value.
        let bytes = serde_json::to_vec(&s).expect("serialize snapshot");
        let back: DocumentSnapshot = serde_json::from_slice(&bytes).expect("deserialize snapshot");
        assert_eq!(s, back);
        // Re-serializing the round-tripped value is byte-identical.
        assert_eq!(serde_json::to_vec(&back).unwrap(), bytes);
        // camelCase field names in the wire form.
        let text = serde_json::to_string(&s).unwrap();
        assert!(text.contains("\"docId\""));
        assert!(text.contains("\"bitmapToken\""));
        assert!(text.contains("\"pixelVersion\""));
        // Unset bitmap_token is skipped (not present).
        assert!(!text.contains("\"bitmapToken\":null"));
    }

    // ── Registry integration: record_snapshot + atomic undo/redo ──

    fn open_doc_with_layer(r: &mut PixelStoreRegistry, doc: &str, layer: &str, w: u32, h: u32) {
        r.open_document(doc);
        r.add_layer(doc, layer, w, h, vec![0u8; (w * h * 4) as usize])
            .unwrap();
    }

    fn layer_snap(
        layer: &str,
        w: u32,
        h: u32,
        token: &str,
        epoch: u64,
        pixel_version: u64,
    ) -> LayerSnapshot {
        let mut ls = LayerSnapshot::new(layer, w, h).with_bitmap_token(token);
        ls.epoch = epoch;
        ls.pixel_version = pixel_version;
        ls
    }

    #[test]
    fn record_snapshot_bumps_version_once_and_roundtrips_atomic() {
        let mut r = PixelStoreRegistry::new();
        open_doc_with_layer(&mut r, "docA", "L1", 16, 16);

        // BOTH states are stored (matching the Pixel before/after pattern): undo
        // returns `before`, redo returns `after` — the TWO DIFFERENT directions.
        let before = DocumentSnapshot::new("docA", 1).with_layer(layer_snap(
            "L1",
            16,
            16,
            "tok-before",
            7,
            9,
        ));
        let after = DocumentSnapshot::new("docA", 1).with_layer(layer_snap(
            "L1",
            16,
            16,
            "tok-after",
            8,
            10,
        ));
        r.record_snapshot("docA", before.clone(), after.clone())
            .expect("record_snapshot");

        // version bumps ONCE, cursor advances exactly once.
        assert_eq!(r.get_history_version("docA"), Some(1), "version bumps once");
        assert_eq!(
            r.get_history_cursor("docA"),
            Some(1),
            "cursor advanced once"
        );

        // Undo returns the BEFORE state: metadata (width/height/epoch/version)
        // AND the opaque bitmap token (pixel reference) come back from ONE entry.
        let undo = r.undo_snapshot("docA").expect("undo_snapshot returns Some");
        assert_eq!(undo.doc_id, "docA");
        assert_eq!(undo.version, 1);
        let ls = &undo.layers[0];
        assert_eq!(ls.layer_id, "L1");
        assert_eq!(ls.width, 16);
        assert_eq!(ls.height, 16);
        assert_eq!(
            ls.bitmap_token.as_deref(),
            Some("tok-before"),
            "undo returns the BEFORE token"
        );
        assert_eq!(ls.epoch, 7, "epoch preserved");
        assert_eq!(ls.pixel_version, 9, "pixel_version preserved");
        assert_eq!(r.get_history_version("docA"), Some(2), "undo bumps version");
        assert_eq!(r.get_history_cursor("docA"), Some(0));

        // Redo re-applies the AFTER state (metadata + token) atomically — the
        // CORRECT direction, distinct from undo.
        let redo = r.redo_snapshot("docA").expect("redo_snapshot returns Some");
        assert_eq!(redo, after, "redo returns the AFTER snapshot");
        assert_ne!(undo, redo, "undo != redo (before vs after)");
        assert_eq!(redo.layers[0].bitmap_token.as_deref(), Some("tok-after"));
        assert_eq!(r.get_history_version("docA"), Some(3), "redo bumps version");
        assert_eq!(r.get_history_cursor("docA"), Some(1));
    }

    #[test]
    fn snapshot_undo_restores_metadata_and_pixel_ref_from_one_entry() {
        let mut r = PixelStoreRegistry::new();
        open_doc_with_layer(&mut r, "docA", "L1", 32, 32);

        let before =
            DocumentSnapshot::new("docA", 1).with_layer(layer_snap("L1", 32, 32, "tok-42", 3, 3));
        let after =
            DocumentSnapshot::new("docA", 1).with_layer(layer_snap("L1", 32, 32, "tok-75", 4, 4));
        r.record_snapshot("docA", before, after)
            .expect("record_snapshot");

        // The atomic undo yields a single snapshot that carries BOTH the layer
        // metadata AND the pixel reference (no dual-store split) — and it is the
        // BEFORE direction.
        let undo = r.undo_snapshot("docA").expect("undo");
        let ls = &undo.layers[0];
        assert_eq!(ls.layer_id, "L1");
        assert_eq!((ls.width, ls.height), (32, 32));
        assert_eq!(
            ls.bitmap_token.as_deref(),
            Some("tok-42"),
            "undo carries the BEFORE pixel ref"
        );
        assert_eq!(ls.epoch, 3);
        assert_eq!(ls.pixel_version, 3);
    }

    #[test]
    fn record_snapshot_invalid_doc_errors() {
        let mut r = PixelStoreRegistry::new();
        let before = DocumentSnapshot::new("missing", 1).with_layer(LayerSnapshot::new("L", 8, 8));
        let after = DocumentSnapshot::new("missing", 2).with_layer(LayerSnapshot::new("L", 8, 8));
        let err = r.record_snapshot("missing", before, after).unwrap_err();
        assert!(err.contains("document not open"), "err: {err}");
    }

    #[test]
    fn record_snapshot_doc_id_mismatch_errors() {
        let mut r = PixelStoreRegistry::new();
        r.open_document("docA");
        let ls = || LayerSnapshot::new("L", 8, 8);
        // before mismatches -> Err.
        let err = r
            .record_snapshot("docA", DocumentSnapshot::new("docB", 1).with_layer(ls()), {
                let mut a = DocumentSnapshot::new("docA", 2);
                a = a.with_layer(ls());
                a
            })
            .unwrap_err();
        assert!(err.contains("before.doc_id mismatch"), "err: {err}");
        // after mismatches -> Err (the check runs on BOTH sides).
        let err2 = r
            .record_snapshot(
                "docA",
                {
                    let mut b = DocumentSnapshot::new("docA", 1);
                    b = b.with_layer(ls());
                    b
                },
                DocumentSnapshot::new("docB", 2).with_layer(ls()),
            )
            .unwrap_err();
        assert!(err2.contains("after.doc_id mismatch"), "err: {err2}");
        // No history was mutated by the rejected calls.
        assert_eq!(r.get_history_cursor("docA"), Some(0));
    }

    #[test]
    fn record_snapshot_missing_layer_is_noop() {
        let mut r = PixelStoreRegistry::new();
        r.open_document("docA");
        // The snapshot references a layer with no canonical buffer; recording
        // must not error (missing layer no-op) and version still bumps once.
        let before = DocumentSnapshot::new("docA", 1).with_layer(layer_snap(
            "nonexistent",
            8,
            8,
            "tok-x",
            1,
            1,
        ));
        let after = DocumentSnapshot::new("docA", 2).with_layer(layer_snap(
            "nonexistent",
            8,
            8,
            "tok-y",
            2,
            2,
        ));
        r.record_snapshot("docA", before.clone(), after.clone())
            .expect("missing layer no-ops");
        assert_eq!(
            r.get_history_version("docA"),
            Some(1),
            "version still bumps once"
        );
        let undo = r.undo_snapshot("docA").expect("undo returns snapshot");
        assert_eq!(undo.layers[0].bitmap_token.as_deref(), Some("tok-x"));
        let redo = r.redo_snapshot("docA").expect("redo returns snapshot");
        assert_eq!(redo.layers[0].bitmap_token.as_deref(), Some("tok-y"));
    }

    #[test]
    fn mixed_pixel_then_snapshot_undo_redo_steps_unified_cursor() {
        let mut r = PixelStoreRegistry::new();
        let n = 64u32;
        open_doc_with_layer(&mut r, "docA", "L", n, n);

        // Pixel entry first (version 1).
        let cur = r.get_layer("docA", "L").unwrap().pixels.clone();
        let after_data: Vec<u8> = cur.iter().map(|b| b ^ 7).collect();
        let before_patch = TilePatch {
            x: 0,
            y: 0,
            w: n as usize,
            h: n as usize,
            data: cur,
        };
        let after_patch = TilePatch {
            x: 0,
            y: 0,
            w: n as usize,
            h: n as usize,
            data: after_data,
        };
        let (_b, _e, ver) = r
            .apply_pixel_patch("docA", "L", vec![before_patch], vec![after_patch])
            .expect("pixel commit");
        assert_eq!(ver, 1);

        // Snapshot entry second (version 2): stores BOTH before and after.
        let before_snap =
            DocumentSnapshot::new("docA", 1).with_layer(layer_snap("L", n, n, "tok-before", 1, 1));
        let after_snap =
            DocumentSnapshot::new("docA", 2).with_layer(layer_snap("L", n, n, "tok-after", 2, 2));
        r.record_snapshot("docA", before_snap.clone(), after_snap.clone())
            .expect("snapshot record");
        assert_eq!(r.get_history_cursor("docA"), Some(2));
        assert_eq!(r.get_history_version("docA"), Some(2));

        // Undo the Snapshot (metadata + token) -> BEFORE, then the Pixel (tiles).
        let u1 = r
            .undo_snapshot("docA")
            .expect("undo snapshot returns metadata+token");
        assert_eq!(u1.layers[0].bitmap_token.as_deref(), Some("tok-before"));
        assert_eq!(r.get_history_cursor("docA"), Some(1));
        let u2 = r.undo_pixel("docA").expect("undo pixel returns tiles");
        assert_eq!(u2.0, "L");
        assert_eq!(r.get_history_cursor("docA"), Some(0));

        // Redo the Pixel, then the Snapshot — unified cursor steps both.
        let r1 = r.redo_pixel("docA").expect("redo pixel");
        assert_eq!(r1.0, "L");
        assert_eq!(r.get_history_cursor("docA"), Some(1));
        let r2 = r.redo_snapshot("docA").expect("redo snapshot re-applies");
        assert_eq!(r2, after_snap, "redo re-applies the AFTER snapshot");
        assert_eq!(r.get_history_cursor("docA"), Some(2));

        // Version is strictly monotonic across every undo/redo step.
        assert_eq!(r.get_history_version("docA"), Some(6));
    }

    // C1: `undo_snapshot` on a PIXEL tip -> None + cursor unchanged (the
    // `_ => None` no-move branch — the dispatch contract foundation: a caller
    // must route by `tip_payload_kind()` before driving `undo_snapshot`).
    #[test]
    fn c1_undo_snapshot_on_pixel_tip_is_none_and_cursor_unchanged() {
        let mut r = PixelStoreRegistry::new();
        let n = 8u32;
        open_doc_with_layer(&mut r, "docA", "L", n, n);
        let cur = r.get_layer("docA", "L").unwrap().pixels.clone();
        let after_data: Vec<u8> = cur.iter().map(|b| b ^ 3).collect();
        let bp = TilePatch {
            x: 0,
            y: 0,
            w: n as usize,
            h: n as usize,
            data: cur,
        };
        let ap = TilePatch {
            x: 0,
            y: 0,
            w: n as usize,
            h: n as usize,
            data: after_data,
        };
        r.apply_pixel_patch("docA", "L", vec![bp], vec![ap])
            .expect("pixel commit");
        assert_eq!(r.get_history_cursor("docA"), Some(1));
        // The tip is a Pixel entry; the snapshot undo path must refuse it
        // WITHOUT moving the cursor.
        assert!(r.undo_snapshot("docA").is_none());
        assert_eq!(
            r.get_history_cursor("docA"),
            Some(1),
            "cursor unchanged on non-snapshot tip"
        );
    }

    // C2: undo (snapshot) then record a NEW snapshot -> truncates the redo
    // branch (redo stack gone / redo returns None).
    #[test]
    fn c2_undo_then_record_snapshot_truncates_redo_branch() {
        let mut r = PixelStoreRegistry::new();
        let n = 8u32;
        open_doc_with_layer(&mut r, "docA", "L", n, n);
        // pixel entry at index 0
        let cur = r.get_layer("docA", "L").unwrap().pixels.clone();
        let after_data: Vec<u8> = cur.iter().map(|b| b ^ 1).collect();
        let bp = TilePatch {
            x: 0,
            y: 0,
            w: n as usize,
            h: n as usize,
            data: cur,
        };
        let ap = TilePatch {
            x: 0,
            y: 0,
            w: n as usize,
            h: n as usize,
            data: after_data,
        };
        r.apply_pixel_patch("docA", "L", vec![bp], vec![ap])
            .expect("pixel commit");
        // snapshot entry at index 1
        let before =
            DocumentSnapshot::new("docA", 1).with_layer(layer_snap("L", n, n, "tok-b1", 1, 1));
        let after =
            DocumentSnapshot::new("docA", 2).with_layer(layer_snap("L", n, n, "tok-a1", 2, 2));
        r.record_snapshot("docA", before, after)
            .expect("snapshot record");
        assert_eq!(r.get_history_cursor("docA"), Some(2));
        // undo the snapshot -> cursor 1 (the snapshot is now the redo region).
        r.undo_snapshot("docA").expect("undo snapshot");
        assert_eq!(r.get_history_cursor("docA"), Some(1));
        // record a NEW snapshot -> truncates the redo branch (the undone
        // snapshot is severed; redo stack is gone).
        let before2 =
            DocumentSnapshot::new("docA", 1).with_layer(layer_snap("L", n, n, "tok-b2", 3, 3));
        let after2 =
            DocumentSnapshot::new("docA", 2).with_layer(layer_snap("L", n, n, "tok-a2", 4, 4));
        r.record_snapshot("docA", before2, after2)
            .expect("new snapshot record");
        assert!(
            r.redo_snapshot("docA").is_none(),
            "redo truncated after new snapshot"
        );
        assert_eq!(r.get_history_cursor("docA"), Some(2));
        // redo_pixel has nothing to redo either (branch severed at the cursor).
        assert!(r.redo_pixel("docA").is_none());
    }
}
