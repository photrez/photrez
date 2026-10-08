// SPDX-License-Identifier: AGPL-3.0-or-later

//! Raw-bytes transport for the canonical region write.
//!
//! Companion to `rust_pixels_write_region` (paint_parity_cmds.rs). Its reason to
//! exist is measured, not stylistic: the JSON region write was decomposed on the
//! shipped WebView at 4096^2 with a 3254x208 dirty rect (see
//! commitFrameYield.ts) and read readRect 6.0, encode 19.5, parse 1.3, decode
//! 16.2, apply 8.3 ms — the remaining round trip sits INSIDE the invoke, where
//! Tauri serialises the reply's byte arrays. The repair follows the platform's
//! own documented mechanism ("Returning Array Buffers",
//! https://v2.tauri.app/develop/calling-rust/): the reply is ONE
//! `tauri::ipc::Response` ArrayBuffer and the request RAW bytes, so no per-byte
//! JSON number is built on either side.
//!
//! WIRE FORMAT (bound to `apps/desktop/src/lib/protocol/paintRegionIpc.ts`,
//! change BOTH together - same discipline as the base64 armMirror fixture):
//!
//! ```text
//! REQUEST  (InvokeBody::Raw, content-type application/octet-stream)
//!   header "X-Photrez-Region" JSON {docId, layerId, x, y, w, h, byteLen}
//!   body                      rgba bytes, exactly byteLen long - NOTHING else
//!
//!   The metadata rides the header and is sent ONCE. The length-prefixed frame
//!   below is INTERNAL: `rust_pixels_write_region_raw` re-assembles it from the
//!   header plus the body, and only `process_raw_write` ever sees it. Sending
//!   the frame as the body instead makes its own bytes count as pixels and the
//!   write is rejected - that happened once, in the real app, and the TS test
//!   "sends the rgba bytes as the ENTIRE body" now pins it.
//!
//! INTERNAL FRAME (never on the wire):
//!   [0..8)                    magic "PTZRW01!"
//!   [8..8+4)                  metadata_len (u32 LE)
//!   [8+4 .. 8+4+metadata_len) JSON {docId, layerId, x, y, w, h, byteLen}
//!   [rest]                    rgba bytes, exactly byteLen long
//!
//! RESPONSE (tauri::ipc::Response -> one ArrayBuffer in JS)
//!   [0..8)                    magic "PTZRW01!"
//!   [8..8+4)                  header_len (u32 LE)
//!   [8+4 .. 8+4+header_len)   JSON {epoch, version, tiles:[{x,y,w,h}..]}
//!   [rest]                    tile data blocks in tiles order, each w*h*4 bytes
//! ```
//!
//! Errors stay `Result<_, String>`; every other command here returns errors the
//! same way, so the error envelope shape is unchanged.

use photrez_core::pixel_store::registry;
use tauri::ipc::{InvokeBody, Request, Response};

const MAGIC: &[u8; 8] = b"PTZRW01!";
const MAGIC_LEN: usize = 8;
const LEN_PREFIX_LEN: usize = 4;
/// Offset of the first length prefix (where the magic ends).
pub const LEN_AT: usize = MAGIC_LEN;

fn read_len(buf: &[u8], at: usize, what: &str) -> Result<usize, String> {
    if at + LEN_PREFIX_LEN > buf.len() {
        return Err(format!(
            "raw write_region {what}: payload truncated, no 4-byte length prefix at offset {at}"
        ));
    }
    Ok(u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]) as usize)
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRegionMeta {
    doc_id: String,
    layer_id: String,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    byte_len: usize,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct RawTileHeader {
    pub x: i64,
    pub y: i64,
    pub w: usize,
    pub h: usize,
}

#[derive(serde::Serialize)]
struct RawReplyHeader {
    epoch: u64,
    version: u64,
    tiles: Vec<RawTileHeader>,
}

/// Parse, validate and run one raw region write. Splits off the command wrapper
/// so the parse/validate/build halves are unit-testable without constructing a
/// `tauri::ipc::Request` by hand.
fn process_raw_write(body: &[u8]) -> Result<Vec<u8>, String> {
    if body.len() < LEN_AT || &body[..MAGIC_LEN] != MAGIC {
        return Err("raw write_region payload is missing the PTZRW01! magic".to_string());
    }
    let meta_len = read_len(body, LEN_AT, "metadata")?;
    let meta_end = (LEN_AT + LEN_PREFIX_LEN)
        .checked_add(meta_len)
        .ok_or_else(|| "raw write_region metadata length overflows".to_string())?;
    let meta: RawRegionMeta = serde_json::from_slice(
        body.get(LEN_AT + LEN_PREFIX_LEN..meta_end)
            .ok_or_else(|| "raw write_region metadata extends past the payload".to_string())?,
    )
    .map_err(|e| format!("raw write_region metadata is not valid JSON: {e}"))?;
    let rgba = body
        .get(meta_end..)
        .ok_or_else(|| "raw write_region rgba bytes extend past the payload".to_string())?;
    if rgba.len() != meta.byte_len {
        return Err(format!(
            "rgba byte count mismatch: body carries {carried}, header declares {declared}",
            carried = rgba.len(),
            declared = meta.byte_len
        ));
    }
    if meta.w < 0 || meta.h < 0 {
        return Err(format!("negative region dimensions: {}x{}", meta.w, meta.h));
    }
    if meta.x < 0 || meta.y < 0 {
        return Err(format!("negative region origin: ({}, {})", meta.x, meta.y));
    }
    if meta.w == 0 || meta.h == 0 {
        return Err(format!("empty region dimensions: {}x{}", meta.w, meta.h));
    }
    let expected = (meta.w as usize)
        .checked_mul(meta.h as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "region dimensions overflow byte count".to_string())?;
    if rgba.len() != expected {
        return Err(format!(
            "write region rgba is {carried} bytes; expected {expected} for {w}x{h}",
            carried = rgba.len(),
            w = meta.w,
            h = meta.h
        ));
    }
    let mut reg = registry();
    let reg = reg.get_or_insert_with(Default::default);
    let (_pre_image, after, epoch, version) = match reg.write_region(
        &meta.doc_id,
        &meta.layer_id,
        meta.x,
        meta.y,
        meta.w as usize,
        meta.h as usize,
        rgba.to_vec(),
    ) {
        Some(result) => result,
        None => {
            // The store collapses every failed write into one
            // None. Name the cause the caller can act on: a
            // missing layer is an identity problem, a present
            // layer that refused the region is a geometry one.
            // Negative, empty and size-mismatched regions were
            // rejected above, and the store builds in-bounds
            // tile patches from a fitting region, so a live
            // layer failing here means the region exceeded it.
            if reg.get_layer(&meta.doc_id, &meta.layer_id).is_some() {
                return Err(format!(
                    "region out of bounds: layer {} rejected {}x{} at ({}, {})",
                    meta.layer_id, meta.w, meta.h, meta.x, meta.y
                ));
            }
            return Err(format!("layer not initialized: {}", meta.layer_id));
        }
    };

    // Reply: header JSON first, then every tile's data block. Zero-copy view of
    // the written tiles is impossible here (the registry owns them), so each
    // block is copied once - the same single copy the base64 path pays, minus
    // its encode/decode and per-byte JSON expansion.
    let tiles: Vec<RawTileHeader> = after
        .iter()
        .map(|t| RawTileHeader {
            x: t.x,
            y: t.y,
            w: t.w,
            h: t.h,
        })
        .collect();
    let header = serde_json::to_vec(&RawReplyHeader {
        epoch,
        version,
        tiles,
    })
    .map_err(|e| format!("raw write_region reply header failed to serialise: {e}"))?;
    let mut reply = Vec::with_capacity(
        LEN_AT + LEN_PREFIX_LEN + header.len() + after.iter().map(|t| t.data.len()).sum::<usize>(),
    );
    reply.extend_from_slice(MAGIC);
    reply.extend_from_slice(&(header.len() as u32).to_le_bytes());
    reply.extend_from_slice(&header);
    for t in &after {
        reply.extend_from_slice(&t.data);
    }
    Ok(reply)
}

/// Pure core of the capability probe: echoes the first byte of the request
/// back inside the same frame shape the region write uses, so a caller that can
/// read this reply has both directions proven without touching the store.
fn echo_frame(first_byte: Option<u8>) -> Vec<u8> {
    let mut reply = Vec::with_capacity(MAGIC_LEN + LEN_PREFIX_LEN + 1);
    reply.extend_from_slice(MAGIC);
    reply.extend_from_slice(&1u32.to_le_bytes());
    reply.push(first_byte.unwrap_or(0xAB));
    reply
}

/// Read-only capability probe: answers raw when the webview can deliver this
/// command's body as bytes (i.e. it actually reaches this parser), so the TS
/// caller can pin its transport choice to evidence instead of assuming. OFF the
/// byte-writer census: it never touches the store.
#[tauri::command]
pub fn rust_pixels_raw_echo(request: Request<'_>) -> Result<Response, String> {
    match request.body() {
        InvokeBody::Raw(bytes) => Ok(Response::new(echo_frame(bytes.first().copied()))),
        InvokeBody::Json(_) => Err("raw echo requires a binary payload, not JSON".to_string()),
    }
}

/// The raw-bytes canonical region write (one `Pixel` history entry, same as the
/// base64 command). Registered in main.rs next to its base64 sibling.
///
/// THE WIRE CONTRACT: the invoke body carries metadata via the
/// `X-Photrez-Region` header and the RGBA bytes as a raw body; read with
/// `Request::headers`, `Request::body` respectively. Errors arrive as the usual
/// JSON string envelope. This pairing is asserted by the shared TS
/// `paintRegionIpc` probe module, which also holds a fallback path onto the
/// base64 command so the two sides cannot drift apart silently.
#[tauri::command]
pub fn rust_pixels_write_region_raw(request: Request<'_>) -> Result<Response, String> {
    // The metadata rides the `X-Photrez-Region` header (header names are
    // lowercase-normalised per Tauri's HeaderMap). Body carries the bytes only.
    let header_value = request
        .headers()
        .get("x-photrez-region")
        .ok_or_else(|| "raw write_region is missing the X-Photrez-Region header".to_string())?
        .to_str()
        .map_err(|e| format!("raw write_region header is not valid UTF-8: {e}"))?
        .to_string();
    let bytes = match request.body() {
        InvokeBody::Raw(bytes) => bytes,
        InvokeBody::Json(_) => {
            return Err("raw write_region requires a binary payload, not JSON".to_string())
        }
    };

    // Assemble the wire frame this module's parser expects: magic + meta len +
    // JSON metadata + byte splice.
    if header_value.is_empty() {
        return Err("raw write_region header X-Photrez-Region is empty".to_string());
    }
    let body_frame: Vec<u8> = {
        let meta: RawRegionMeta = serde_json::from_str(&header_value)
            .map_err(|e| format!("raw write_region metadata is not valid JSON: {e}"))?;
        let meta_json = serde_json::to_vec(&meta)
            .map_err(|e| format!("raw write_region metadata re-serialisation failed: {e}"))?;
        let mut frame = Vec::with_capacity(LEN_AT + LEN_PREFIX_LEN + meta_json.len() + bytes.len());
        frame.extend_from_slice(MAGIC);
        frame.extend_from_slice(&(meta_json.len() as u32).to_le_bytes());
        frame.extend_from_slice(&meta_json);
        frame.extend_from_slice(&bytes);
        frame
    };
    let reply = process_raw_write(&body_frame)?;
    Ok(Response::new(reply))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint_parity_cmds::TEST_REGISTRY_LOCK;

    /// Wire bytes: magic | len(prefix) | meta json | rgba
    fn wire_bytes(meta: &RawRegionMeta, rgba: &[u8]) -> Vec<u8> {
        let meta_json = serde_json::to_vec(meta).unwrap();
        let mut out = Vec::with_capacity(LEN_AT + LEN_PREFIX_LEN + meta_json.len() + rgba.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(meta_json.len() as u32).to_le_bytes());
        out.extend_from_slice(&meta_json);
        out.extend_from_slice(rgba);
        out
    }

    /// One 4x2 region: 8 pixels x 4 bytes = 32 bytes. Two tiles (left half / right half).
    fn grid_meta() -> RawRegionMeta {
        RawRegionMeta {
            doc_id: "doc".into(),
            layer_id: "layer".into(),
            x: 0,
            y: 0,
            w: 4,
            h: 2,
            byte_len: 32,
        }
    }

    #[test]
    fn rejects_missing_magic() {
        let err = process_raw_write(&[0u8; 64]).err().unwrap();
        assert!(err.contains("missing the PTZRW01! magic"), "got: {err}");
    }

    #[test]
    fn rejects_truncated_length_prefix() {
        let mut bad = Vec::new();
        bad.extend_from_slice(MAGIC);
        bad.extend_from_slice(&[0, 0]);
        let err = process_raw_write(&bad).err().unwrap();
        assert!(err.contains("truncated"), "got: {err}");
    }

    #[test]
    fn rejects_invalid_metadata_json() {
        let mut bad = Vec::new();
        bad.extend_from_slice(MAGIC);
        bad.extend_from_slice(&2u32.to_le_bytes());
        bad.extend_from_slice(b"{}");
        let err = process_raw_write(&bad).err().unwrap();
        assert!(err.contains("metadata"), "got: {err}");
    }

    #[test]
    fn rejects_metadata_length_past_the_payload() {
        // Hostile meta_len: u32::MAX. The length-prefix arithmetic
        // is honest on 64-bit (12 + u32::MAX fits a usize), but
        // the slice it names runs far past the payload, so the
        // read must be refused with the field named instead of
        // panicking or reading foreign memory.
        let mut bad = Vec::new();
        bad.extend_from_slice(MAGIC);
        bad.extend_from_slice(&u32::MAX.to_le_bytes());
        bad.extend_from_slice(b"{}");
        let err = process_raw_write(&bad).err().unwrap();
        assert!(
            err.contains("metadata extends past the payload"),
            "got: {err}"
        );
    }

    #[test]
    fn rejects_byte_count_mismatch() {
        let meta = grid_meta();
        let body = wire_bytes(&meta, &[0u8; 28][..]);
        let err = process_raw_write(&body).err().unwrap();
        assert!(err.contains("byte count mismatch"), "got: {err}");
    }

    #[test]
    fn rejects_wrong_region_size_for_byte_count() {
        let meta = grid_meta();
        let mut meta = meta;
        meta.byte_len = 28; // header says 28
        let body = wire_bytes(&meta, &[0u8; 28][..]);
        let err = process_raw_write(&body).err().unwrap();
        assert!(err.contains("expected 32"), "got: {err}");
    }

    #[test]
    fn rejects_negative_dimensions() {
        let mut meta = grid_meta();
        meta.w = -4;
        meta.byte_len = 32;
        let body = wire_bytes(&meta, &[0u8; 32][..]);
        let err = process_raw_write(&body).err().unwrap();
        assert!(err.contains("negative region dimensions"), "got: {err}");
    }

    #[test]
    fn rejects_overflowing_region_dimensions() {
        // Hostile header: w=h=2^40 makes the w*h*4 byte count overflow
        // usize. byteLen 0 and an empty body pass every prior guard, so
        // the byte-count arithmetic is the first thing that must catch it.
        let mut meta = grid_meta();
        meta.w = 1099511627776;
        meta.h = 1099511627776;
        meta.byte_len = 0;
        let body = wire_bytes(&meta, &[]);
        let err = process_raw_write(&body).err().unwrap();
        assert!(
            err.contains("region dimensions overflow byte count"),
            "got: {err}"
        );
    }

    #[test]
    fn rejects_zero_width_region() {
        let mut meta = grid_meta();
        meta.w = 0;
        meta.byte_len = 0;
        let body = wire_bytes(&meta, &[]);
        let err = process_raw_write(&body).err().unwrap();
        assert!(err.contains("empty region dimensions"), "got: {err}");
    }

    #[test]
    fn rejects_zero_height_region() {
        let mut meta = grid_meta();
        meta.h = 0;
        meta.byte_len = 0;
        let body = wire_bytes(&meta, &[]);
        let err = process_raw_write(&body).err().unwrap();
        assert!(err.contains("empty region dimensions"), "got: {err}");
    }

    #[test]
    fn rejects_negative_x() {
        let mut meta = grid_meta();
        meta.x = -1;
        let body = wire_bytes(&meta, &[0u8; 32][..]);
        let err = process_raw_write(&body).err().unwrap();
        assert!(err.contains("negative region origin"), "got: {err}");
    }

    #[test]
    fn rejects_negative_y() {
        let mut meta = grid_meta();
        meta.y = -1;
        let body = wire_bytes(&meta, &[0u8; 32][..]);
        let err = process_raw_write(&body).err().unwrap();
        assert!(err.contains("negative region origin"), "got: {err}");
    }

    #[test]
    fn writes_and_builds_reply_against_a_live_store() {
        // Serializes against every other registry-touching test:
        // the process-global registry is shared, and a parallel
        // reset in another test would wipe this test's documents
        // mid-run (same contract as paint_parity_cmds.rs tests).
        let _g = TEST_REGISTRY_LOCK.lock().unwrap();
        // Wiping the store between tests would need a reset command; instead
        // this test uses a unique layer id so tests stay independent.
        let layer_id = "reply-layer";
        let meta = RawRegionMeta {
            doc_id: "reply-doc".into(),
            layer_id: layer_id.into(),
            x: 0,
            y: 0,
            w: 2,
            h: 2,
            byte_len: 16,
        };
        {
            let mut reg = registry();
            let reg = reg.get_or_insert_with(Default::default); // Seed an 8x8 layer with zeros: init_layer asserts the byte count is
                                                                // exactly w*h*4, so an empty Vec would panic in the store.
            reg.add_layer("reply-doc", layer_id, 8, 8, vec![0u8; 8 * 8 * 4])
                .unwrap();
        }
        let rgba = vec![10u8; 16];
        let body = wire_bytes(&meta, &rgba);
        let reply = process_raw_write(&body).unwrap();

        // Reply: magic | header_len | header JSON | data blocks
        assert_eq!(&reply[..MAGIC_LEN], MAGIC, "reply magic");
        let header_len = u32::from_le_bytes([
            reply[MAGIC_LEN],
            reply[MAGIC_LEN + 1],
            reply[MAGIC_LEN + 2],
            reply[MAGIC_LEN + 3],
        ]) as usize;
        let header: serde_json::Value = serde_json::from_slice(
            &reply[LEN_AT + LEN_PREFIX_LEN..LEN_AT + LEN_PREFIX_LEN + header_len],
        )
        .unwrap();
        assert_eq!(
            header["tiles"].as_array().unwrap().len(),
            1,
            "2x2 region fits one 256 tile"
        );
        let epoch = header["epoch"].as_u64().unwrap();
        let version = header["version"].as_u64().unwrap();
        assert!(epoch >= 1, "write advances the epoch");
        assert!(version >= 1, "write advances the version");
        // Data block: the tile patch carries the TILE's full bytes (an 8x8 layer
        // is one tile: 256 bytes), the same semantics the base64 reply has.
        let data = &reply[LEN_AT + LEN_PREFIX_LEN + header_len..];
        assert_eq!(
            data.len(),
            8 * 8 * 4,
            "one full-tile block for an 8x8 layer"
        );
        // The 2x2 write at the origin shows up at rows 0-1, cols 0-1.
        assert!(
            data[..8].iter().all(|&b| b == 10),
            "row 0, cols 0-1 written"
        );
        assert!(
            data[8..32].iter().all(|&b| b == 0),
            "row 0, cols 2-7 untouched"
        );
        assert!(
            data[32..40].iter().all(|&b| b == 10),
            "row 1, cols 0-1 written"
        );
        assert!(data[40..].iter().all(|&b| b == 0), "rows 2-7 untouched");
    }

    #[test]
    fn echo_frame_returns_magic_length_and_the_probe_byte() {
        let reply = echo_frame(Some(0x5A));
        assert_eq!(&reply[..MAGIC_LEN], MAGIC);
        assert_eq!(
            u32::from_le_bytes([
                reply[MAGIC_LEN],
                reply[MAGIC_LEN + 1],
                reply[MAGIC_LEN + 2],
                reply[MAGIC_LEN + 3]
            ]),
            1
        );
        assert_eq!(reply[MAGIC_LEN + LEN_PREFIX_LEN], 0x5A);
        assert_eq!(echo_frame(None)[MAGIC_LEN + LEN_PREFIX_LEN], 0xAB);
    }

    #[test]
    fn rejects_region_out_of_bounds_for_a_live_layer() {
        // Seeds its OWN layer: the write below must fail because
        // the REGION exceeds the layer, not because the layer is
        // missing. The old form reused another test's layer and
        // asserted the combined "not initialized or out of bounds"
        // string, which cannot tell the two causes apart.
        let _g = TEST_REGISTRY_LOCK.lock().unwrap();
        let doc_id = "oob-doc";
        let layer_id = "oob-layer";
        {
            let mut reg = registry();
            let reg = reg.get_or_insert_with(Default::default);
            reg.add_layer(doc_id, layer_id, 8, 8, vec![0u8; 8 * 8 * 4])
                .unwrap();
        }
        let meta = RawRegionMeta {
            doc_id: doc_id.into(),
            layer_id: layer_id.into(),
            x: 100,
            y: 100,
            w: 2,
            h: 2,
            byte_len: 16,
        };
        let body = wire_bytes(&meta, &[0u8; 16][..]);
        let err = process_raw_write(&body).err().unwrap();
        assert!(err.contains("region out of bounds"), "got: {err}");
        assert!(!err.contains("not initialized"), "got: {err}");
    }
}
