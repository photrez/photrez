// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Read-only pixel-store byte probe. Registered as `rust_pixels_store_bytes`.
//
// Contract: `rust_pixels_store_bytes` returns exactly one `PixelStoreBytes` for
// one document - the row-major mirror bytes summed from each layer's buffer
// length, plus the tile-graph bytes counted once per distinct `Arc<[u8]>` block,
// plus the shared/private split of that tile graph. The accessor behind it takes
// `&self` only - no buffer write, epoch bump, cursor pop, version bump or
// `Arc<StateNode>` mutation - so two calls on an unchanged document are
// byte-identical. Empty, whitespace-only, unknown and oversize doc_id reject with
// a bare string (Tauri surfaces a Rust Err(String) as a bare string); they never
// resolve to a zero-byte report, which would read as "this document is free".
//
// It reports no pixel bytes. It reports LENGTHS, so a caller learns the store's
// footprint without a single pixel crossing the boundary.

use photrez_core::pixel_store::{registry, PixelStoreBytes};

/// Longest `doc_id` (UTF-8 bytes) this probe accepts. Production ids are
/// `doc-<uuid>` (41 bytes); 256 keeps headroom while rejecting garbage before it
/// reaches the process-global registry lock. Matches the threshold the history
/// probes use, so one id is accepted or rejected the same way by every probe.
const MAX_DOC_ID_BYTES: usize = 256;

/// Read-only byte-footprint probe: never mutates the store, and every invalid
/// `doc_id` rejects with a bare string instead of resolving a zero-byte report.
#[tauri::command]
pub fn rust_pixels_store_bytes(doc_id: String) -> Result<PixelStoreBytes, String> {
    if doc_id.trim().is_empty() {
        return Err("empty doc_id".to_string());
    }
    if doc_id.len() > MAX_DOC_ID_BYTES {
        return Err(format!(
            "oversize doc_id: {} bytes (max {MAX_DOC_ID_BYTES})",
            doc_id.len()
        ));
    }
    let reg = registry();
    let reg = reg
        .as_ref()
        .ok_or_else(|| "pixel store not initialized".to_string())?;
    reg.get_store_bytes(&doc_id)
        .ok_or_else(|| format!("document not open: {doc_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint_parity_cmds::{
        apply_tile_patch, rust_pixels_init, rust_pixels_open_document, TilePatchWire,
        TEST_REGISTRY_LOCK,
    };

    fn wire(x: i64, y: i64, w: usize, h: usize, data: Vec<u8>) -> TilePatchWire {
        TilePatchWire { x, y, w, h, data }
    }

    #[test]
    fn store_bytes_probe_is_read_only_and_repeatable() {
        let _g = TEST_REGISTRY_LOCK.lock().unwrap();
        *registry() = None;

        let doc = "bytes-doc-1".to_string();
        let layer = "bytes-layer-1".to_string();
        rust_pixels_open_document(doc.clone());
        rust_pixels_init(doc.clone(), layer.clone(), 4, 4, vec![0; 4 * 4 * 4]).expect("init");
        let before = vec![wire(0, 0, 4, 4, vec![0; 4 * 4 * 4])];
        let after = vec![wire(0, 0, 4, 4, vec![9; 4 * 4 * 4])];
        apply_tile_patch(doc.clone(), layer.clone(), before, after).expect("patch");

        let cursor_before = reg_cursor(&doc);
        let version_before = reg_version(&doc);
        let epoch_before = reg_epoch(&doc, &layer);

        let first = rust_pixels_store_bytes(doc.clone()).expect("first bytes read");
        let second = rust_pixels_store_bytes(doc.clone()).expect("second bytes read");
        assert_eq!(first, second, "two identical read-only calls");

        // 4x4 RGBA = 64 bytes in the mirror. The commit re-tiled the layer's only
        // tile, so the tile graph is two 64-byte tiles with one owner each:
        // nothing is shared on a single-tile layer.
        assert_eq!(first.layer_count, 1);
        assert_eq!(first.row_major_bytes, 64, "4*4*4 = 64 bytes in the mirror");
        assert_eq!(
            first.tile_graph.state_count, 2,
            "anchor plus committed state"
        );
        assert_eq!(first.tile_graph.tile_count, 2);
        assert_eq!(first.tile_graph.total_bytes, 128);
        assert_eq!(
            first.tile_graph.shared_bytes, 0,
            "one owner per tile: a single-tile layer has nothing to share"
        );
        assert_eq!(first.total_bytes, 192, "64 mirror + 128 tile graph");

        assert_eq!(reg_cursor(&doc), cursor_before, "cursor must not advance");
        assert_eq!(reg_version(&doc), version_before, "version must not bump");
        assert_eq!(
            reg_epoch(&doc, &layer),
            epoch_before,
            "no canonical mutation, so the epoch holds"
        );
    }

    /// The sharing term has to be non-zero somewhere, or a probe reporting it is
    /// reporting a constant zero. A 300x300 layer is 2x2 tiles, so a commit inside
    /// the first one leaves three tiles keeping the anchor's identity:
    #[test]
    fn shared_bytes_is_non_zero_where_tiles_really_are_shared() {
        let _g = TEST_REGISTRY_LOCK.lock().unwrap();
        *registry() = None;

        let doc = "bytes-doc-shared".to_string();
        let layer = "bytes-layer-shared".to_string();
        rust_pixels_open_document(doc.clone());
        rust_pixels_init(doc.clone(), layer.clone(), 300, 300, vec![0; 300 * 300 * 4])
            .expect("init");
        let before = vec![wire(0, 0, 300, 300, vec![0; 300 * 300 * 4])];
        let after = vec![wire(0, 0, 300, 300, vec![9; 300 * 300 * 4])];
        apply_tile_patch(doc.clone(), layer.clone(), before, after).expect("patch");

        let bytes = rust_pixels_store_bytes(doc.clone()).expect("bytes read");
        // The wire patch covers the whole layer, so all four tiles are re-tiled and
        // no tile keeps the anchor's identity. The anchor is still the entry's
        // `before`, so it still holds its own four tiles - each with one owner,
        // hence private.
        // 300x300 tiles at 256px: 256x256, 256x44, 44x256, 44x44.
        assert_eq!(
            bytes.tile_graph.tile_count, 8,
            "the anchor's 4 tiles plus 4 fresh re-tiled tiles"
        );
        assert_eq!(
            bytes.tile_graph.shared_bytes, 0,
            "a full-layer patch re-tiles all four tiles, so none keeps its identity"
        );
        assert_eq!(
            bytes.tile_graph.private_bytes, 720_000,
            "360,000 anchor tiles + 262,144 + 45,056 + 45,056 + 7,744 re-tiled tiles"
        );

        // A SUB-TILE region leaves the other three tiles untouched, which is the
        // case the sharing term exists for. Driven through the registry directly
        // so the patch really is 4x4 inside the first 256px tile.
        let doc2 = "bytes-doc-sub".to_string();
        let layer2 = "bytes-layer-sub".to_string();
        rust_pixels_open_document(doc2.clone());
        rust_pixels_init(
            doc2.clone(),
            layer2.clone(),
            300,
            300,
            vec![0; 300 * 300 * 4],
        )
        .expect("init 2");
        {
            let mut reg = registry();
            let reg = reg.as_mut().expect("registry");
            reg.apply_pixel_patch(&doc2, &layer2, vec![], vec![sub_tile()])
                .expect("sub-tile commit");
        }
        let sub = rust_pixels_store_bytes(doc2.clone()).expect("sub-tile bytes");
        // Only the three UNTOUCHED tiles are shared - 45,056 + 45,056 + 7,744.
        // Not the 360,000-byte packed buffer they live in, and not the re-tiled
        // 256x256, which is private in both the anchor and the new state.
        assert_eq!(
            sub.tile_graph.shared_bytes, 97_856,
            "the three untouched tiles only: 45,056 + 45,056 + 7,744"
        );
        assert_eq!(
            sub.tile_graph.private_bytes, 524_288,
            "the anchor's re-tiled 256*256*4 (262,144) plus its fresh copy (262,144)"
        );
        assert_eq!(sub.tile_graph.total_bytes, 622_144);
    }

    /// A zero-dimension document must read as 0 bytes rather than panicking or
    /// inventing a phantom 4. `rust_pixels_init` accepts 0x0 with an empty buffer
    /// because `PixelLayer::new` only asserts `len == w*h*4`.
    #[test]
    fn a_zero_dimension_document_reads_zero_bytes() {
        let _g = TEST_REGISTRY_LOCK.lock().unwrap();
        *registry() = None;

        let doc = "bytes-doc-zero".to_string();
        rust_pixels_open_document(doc.clone());
        rust_pixels_init(doc.clone(), "zero-layer".to_string(), 0, 0, Vec::new())
            .expect("init 0x0");
        let bytes = rust_pixels_store_bytes(doc.clone()).expect("bytes read");
        assert_eq!(bytes.layer_count, 1, "the layer exists");
        assert_eq!(bytes.row_major_bytes, 0, "0x0 RGBA is 0 bytes");
        assert_eq!(bytes.total_bytes, 0);
    }

    #[test]
    fn invalid_doc_ids_reject_with_a_bare_string() {
        let _g = TEST_REGISTRY_LOCK.lock().unwrap();
        *registry() = None;

        for bad in [
            String::new(),
            "   \t\n".to_string(),
            "no-such-document".to_string(),
        ] {
            let err = rust_pixels_store_bytes(bad.clone())
                .expect_err("must reject rather than resolve a zero-byte report");
            assert!(
                !err.is_empty(),
                "a rejection must carry a bare message, not an empty one"
            );
        }
        assert!(rust_pixels_store_bytes("no-such-document".to_string()).is_err());

        // The oversize guard is only proven to WORK if the oversize id would
        // otherwise SUCCEED. An oversize id for a document that was never opened
        // rejects either way, so that assertion is unfalsifiable - open a document
        // whose id is itself oversize, and require the rejection to name the size.
        let oversize = "x".repeat(MAX_DOC_ID_BYTES + 1);
        rust_pixels_open_document(oversize.clone());
        let err = rust_pixels_store_bytes(oversize.clone())
            .expect_err("an oversize doc_id must be rejected even when it IS open");
        assert!(
            err.contains("oversize"),
            "the rejection must come from the length guard, not from a missing document: {err}"
        );

        // Valid length, store never initialized -> also an Err, never Ok(zero).
        assert!(rust_pixels_store_bytes("d".to_string()).is_err());
    }

    /// A document with no layers is a real 0-byte report, not an error - the same
    /// distinction the history probes draw between "no history" and "no document".
    #[test]
    fn an_open_but_empty_document_reports_zero() {
        let _g = TEST_REGISTRY_LOCK.lock().unwrap();
        *registry() = None;

        rust_pixels_open_document("bytes-empty".to_string());
        let empty = rust_pixels_store_bytes("bytes-empty".to_string()).expect("open doc reads");
        assert_eq!(empty.layer_count, 0);
        assert_eq!(empty.row_major_bytes, 0);
        assert_eq!(empty.tile_graph.state_count, 0);
        assert_eq!(empty.total_bytes, 0);
    }

    fn sub_tile() -> photrez_core::pixel_store::TilePatch {
        photrez_core::pixel_store::TilePatch {
            x: 0,
            y: 0,
            w: 4,
            h: 4,
            data: vec![9; 4 * 4 * 4],
        }
    }

    fn reg_cursor(doc: &str) -> Option<usize> {
        let reg = registry();
        reg.as_ref().and_then(|r| r.get_history_cursor(doc))
    }

    fn reg_version(doc: &str) -> Option<u64> {
        let reg = registry();
        reg.as_ref().and_then(|r| r.get_history_version(doc))
    }

    fn reg_epoch(doc: &str, layer: &str) -> Option<u64> {
        let reg = registry();
        reg.as_ref().and_then(|r| r.get_epoch(doc, layer).ok())
    }
}
