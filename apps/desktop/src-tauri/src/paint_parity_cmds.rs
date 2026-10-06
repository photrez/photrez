// Pixel-store and paint-parity Tauri commands. Both populations below are
// REGISTERED in the shipping handler list (main.rs:230-252):
//   PRODUCTION pixel ownership - the `rust_pixels_*` family plus
//   `apply_tile_patch`; a shipped renderer file invokes each, so their argument
//   shapes are wire contracts (`TilePatchWire` below is one).
//   DEV-ONLY parity probes - the `paint_parity_*` / `paint_shadow_*` family, whose
//   only callers are the flag-gated shadow/perf harness (lib/rustShadow.ts,
//   lib/perf/perfAuditDev.ts). None owns pixels.
// `rust_pixels_snapshot_tile` is registered and census-accepted but has no
// production caller. Naming avoids resource_read_* / TileStore by design.

use photrez_core::canonical_tip::{build_canonical_tip, TipSpec};
use photrez_core::paint_parity::{raster_shadow, tiles_for_keys, ParityDab, ParityTip};
use photrez_core::pixel_store::{registry, TilePatch};
use std::collections::HashMap;
use std::sync::Mutex;

// ── C5.1: document-namespaced canonical pixel store ──
// Ownership lives in `photrez_core::pixel_store::PixelStoreRegistry`, keyed by
// (document_id, layer_id). The flat process-global `PIXEL_STORE: HashMap<layerId, PixelLayer>` (no lifecycle)
// is gone. Lifecycle (open/close/add/remove/resize) is driven from TS via the
// rust_pixels_* commands. No TileStore/Rayon/SAB/WebGPU.

/// One tile patch as `apply_tile_patch`'s arguments arrive from the host. The
/// dimensions have two LIVE spellings: `{width, height}` is `TileUploadLike`, the shape
/// every renderer producer builds because the GPU upload path wants it; `w`/`h` is what
/// this command serializes in `TilePatchJson`, so a caller echoing a Rust answer back
/// keeps working. A tile carrying BOTH spellings is rejected as a duplicate field.
///
/// The alias is on the DIMENSION FIELDS, and deliberately not `#[serde(default)]`: a
/// default would turn a missing dimension into `0` - a silently empty tile - instead of
/// rejecting the payload. Renaming the host side would touch `TileUploadLike` and every
/// tile producer, a far wider blast radius for the same acceptance.
#[derive(serde::Deserialize)]
pub struct TilePatchWire {
    pub x: i64,
    pub y: i64,
    #[serde(alias = "width")]
    pub w: usize,
    #[serde(alias = "height")]
    pub h: usize,
    /// base64 of the RGBA bytes. See `b64`. The rename is explicit because Tauri
    /// camelCases a COMMAND's own arguments but not a nested payload struct, so
    /// serde would otherwise look for `data_base64`.
    #[serde(rename = "dataBase64")]
    pub data_base64: String,
}

#[derive(serde::Serialize)]
pub struct TilePatchJson {
    pub key: String,
    pub x: i64,
    pub y: i64,
    pub w: usize,
    pub h: usize,
    /// base64 of the RGBA bytes. See `b64`. The rename is explicit because Tauri
    /// camelCases a COMMAND's own arguments but not a nested payload struct, so
    /// serde would otherwise look for `data_base64`.
    #[serde(rename = "dataBase64")]
    pub data_base64: String,
}

/// Every pixel-bytes field on a Tauri command crosses as BASE64, never as a byte
/// sequence.
///
/// Tauri v2 carries both directions as JSON. Outbound, serde expands a
/// `Vec<u8>` into one JSON number per byte; inbound, the `JSON.stringify`
/// replacer expands a `Uint8Array` argument with `Array.from(val)`, again one
/// element per byte (tauri-2.11.5/scripts/process-ipc-message-fn.js). A 3254x208
/// dirty rect is 2,707,328 bytes, which the number form costs 5,414,721 characters
/// to serialise (589-880 ms in-app) and its 13-tile before/after response costs
/// 27,264,034. A base64 string is copied in one pass: 156 ms for the same bytes.
///
/// This is the same trade the layer seed already makes on `rust_pixels_init`. It
/// is applied to every pixel-bytes field on this path, not just the seed, because
/// leaving one command on the number form would hand the next caller the same
/// stall with nothing in the code to warn them.
///
/// The dev parity harness (`paint_parity_*`) is deliberately NOT converted: it
/// is a flag-gated diagnostic, and its tip bytes are already kept off the
/// transport by the id-based tip registry.
pub(crate) fn b64(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub(crate) fn unb64(text: &str, field: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(text.as_bytes())
        .map_err(|e| format!("{field} is not valid base64: {e}"))
}

/// C5.2: commit/undo/redo now return the post-mutation epoch so the TS derived
/// cache can record exactly which canonical state it reflects.
/// C5.3-A: `version` is the document `DocumentVersion` — the single authoritative
/// history cursor's version, incremented exactly once per accepted pixel command.
#[derive(serde::Serialize)]
pub struct PatchResultJson {
    pub tiles: Vec<TilePatchJson>,
    pub epoch: u64,
    pub version: u64,
}

/// C5.3-A: undo/redo return the affected layer + new epoch + new `DocumentVersion`.
/// Returned even when there is nothing to undo/redo (empty `tiles`).
#[derive(serde::Serialize)]
pub struct PixelHistoryResultJson {
    pub layer_id: String,
    pub tiles: Vec<TilePatchJson>,
    pub epoch: u64,
    pub version: u64,
}

/// Bug 1 fix: canonical-owner brush commit. Returns the exact pre-stroke
/// (`before`) and composited (`after`) tile patches plus the new epoch/version.
/// Rust remains the canonical pixel owner; `after` is the FINAL canonical tile
/// state (composite of existing canonical pixels + dab), not dab-on-white.
#[derive(serde::Serialize)]
pub struct PaintCommitJson {
    pub before: Vec<TilePatchJson>,
    pub after: Vec<TilePatchJson>,
    pub epoch: u64,
    pub version: u64,
}

/// `rust_pixels_write_region`'s reply: the POST-image only.
///
/// No pre-image, because nothing downstream can use one. The write records the
/// pre-image in Rust's own pixel history (the `Pixel` entry `write_region`
/// appends), and undo reads it back from there through `rust_pixels_undo`. The
/// brush memento that the host commits for a stroke is built from the host's own
/// before-image tiles, and every other producer that consumed this reply stored
/// it on an entry marked `rustOwned`, which the undo/redo dispatch deliberately
/// refuses to replay in favour of Rust's bytes. Carrying a second copy of the
/// same pixels across the process boundary cost a whole dirty region's worth of
/// base64 in the response of EVERY stroke.
#[derive(serde::Serialize)]
pub struct WriteRegionJson {
    pub after: Vec<TilePatchJson>,
    pub epoch: u64,
    pub version: u64,
}

fn patch_to_wire(t: &TilePatch) -> TilePatchJson {
    TilePatchJson {
        key: format!("{},{}", t.x / 256, t.y / 256),
        x: t.x,
        y: t.y,
        w: t.w,
        h: t.h,
        data_base64: b64(&t.data),
    }
}

fn wire_to_patch(t: TilePatchWire) -> Result<TilePatch, String> {
    Ok(TilePatch {
        x: t.x,
        y: t.y,
        w: t.w,
        h: t.h,
        data: unb64(&t.data_base64, "tile dataBase64")?,
    })
}

/// Document created/opened: ensure a (possibly empty) pixel namespace exists.
#[tauri::command]
pub fn rust_pixels_open_document(doc_id: String) {
    registry()
        .get_or_insert_with(Default::default)
        .open_document(&doc_id);
}

/// Document closed: release ALL of its pixel storage.
#[tauri::command]
pub fn rust_pixels_close_document(doc_id: String) {
    if let Some(reg) = registry().as_mut() {
        reg.close_document(&doc_id);
    }
}

/// Seed the authoritative buffer for `layer_id` within `doc_id` from the existing
/// layer bytes (one-time; called by TS before the first Rust-owned commit on that layer).
/// Auto-creates the document namespace. Replacement is intentional (failed-commit recovery).
pub(crate) fn seed_layer_bytes(
    doc_id: String,
    layer_id: String,
    width: u32,
    height: u32,
    bytes: Vec<u8>,
) -> Result<(), String> {
    registry()
        .get_or_insert_with(Default::default)
        .add_layer(&doc_id, &layer_id, width, height, bytes)
}

/// Seed the authoritative buffer for `layer_id` within `doc_id`. The seed
/// bytes arrive BASE64-encoded, not as a byte sequence.
///
/// The seed is the whole layer, so on a 4096 x 4096 document it is 67,108,864
/// bytes. Tauri v2 serialises every IPC argument with `JSON.stringify` and a
/// replacer that expands a `Uint8Array` into `Array.from(val)` — one JSON array
/// element per byte (tauri-2.11.5/scripts/process-ipc-message-fn.js). That turns
/// 67 MB of pixels into a 268,435,768-character JSON string and blocks the WebView
/// main thread for ~15 s on the first stroke of a large document.
///
/// A base64 STRING crosses as one string, which `JSON.stringify` copies in a
/// single pass: measured 156 ms against 5,637 ms for the same bytes as a number
/// array. It is also branch-independent — see the note below.
///
/// Why not a raw binary body: Tauri only skips JSON when the whole IPC payload
/// is a typed array, and only on the custom-protocol path
/// (process-ipc-message-fn.js:9-12). On the postMessage fallback — which this app
/// actually runs, verified by the custom-protocol fetch never being called — the
/// ENVELOPE `{cmd, callback, error, options, payload}` is the object handed to the
/// serialiser, so a typed-array payload is expanded back into a number array.
/// Measured on the shipped build: the raw-body seed still produced 268,435,786
/// JSON characters, and Rust rejected the call with "expects a raw binary body".
/// Base64 is correct on both paths.
///
/// Byte-for-byte the same seed reaches `seed_layer_bytes`, so the store, the
/// history stream and every rendered pixel are unchanged — only the transport
/// differs.
#[tauri::command]
pub fn rust_pixels_init(
    doc_id: String,
    layer_id: String,
    width: u32,
    height: u32,
    bytes_base64: String,
) -> Result<(), String> {
    let bytes = unb64(&bytes_base64, "rust_pixels_init bytesBase64")?;
    seed_layer_bytes(doc_id, layer_id, width, height, bytes)
}

/// Drop a layer's pixel storage (layer removed). No-op if absent.
#[tauri::command]
pub fn rust_pixels_remove_layer(doc_id: String, layer_id: String) {
    if let Some(reg) = registry().as_mut() {
        reg.remove_layer(&doc_id, &layer_id);
    }
}

/// Recreate the layer's buffer at new dimensions (layer resized).
#[tauri::command]
pub fn rust_pixels_resize_layer(
    doc_id: String,
    layer_id: String,
    width: u32,
    height: u32,
    bytes_base64: String,
) -> Result<(), String> {
    let bytes = unb64(&bytes_base64, "rust_pixels_resize_layer bytesBase64")?;
    registry()
        .get_or_insert_with(Default::default)
        .resize_layer(&doc_id, &layer_id, width, height, bytes)
}

/// Apply a brush commit: commit a `Pixel` entry on the document's authoritative
/// history stream, write `after` into the canonical buffer, bump epoch, and
/// return the `after` tiles + new epoch + new `DocumentVersion` for TS cache sync.
/// C5.3-A: the history cursor is the SAME `ProtocolEngine` cursor used by metadata
/// history; `PixelLayer.undo_stack`/`redo_stack` are NOT consulted.
#[tauri::command]
pub fn apply_tile_patch(
    doc_id: String,
    layer_id: String,
    before: Vec<TilePatchWire>,
    after: Vec<TilePatchWire>,
) -> Result<PatchResultJson, String> {
    let b: Vec<TilePatch> = before
        .into_iter()
        .map(wire_to_patch)
        .collect::<Result<_, _>>()?;
    let a: Vec<TilePatch> = after
        .into_iter()
        .map(wire_to_patch)
        .collect::<Result<_, _>>()?;
    let mut reg = registry();
    let reg = reg.get_or_insert_with(Default::default);
    let (res_tiles, epoch, version) = reg
        .apply_pixel_patch(&doc_id, &layer_id, b, a)
        .ok_or_else(|| format!("layer not initialized: {layer_id}"))?;
    Ok(PatchResultJson {
        tiles: res_tiles.iter().map(patch_to_wire).collect(),
        epoch,
        version,
    })
}

/// Undo: pop the authoritative history stream, restore pre-stroke state for the
/// affected layer, return `before` tiles + new epoch + new `DocumentVersion`.
/// C5.3-A: routes through `ProtocolEngine.undo_pixel`.
#[tauri::command]
pub fn rust_pixels_undo(
    doc_id: String,
    layer_id: String,
) -> Result<PixelHistoryResultJson, String> {
    let mut reg = registry();
    let reg = reg.get_or_insert_with(Default::default);
    match reg.undo_pixel(&doc_id) {
        Some((lid, tiles, epoch, version)) => Ok(PixelHistoryResultJson {
            layer_id: lid,
            tiles: tiles.iter().map(patch_to_wire).collect(),
            epoch,
            version,
        }),
        None => {
            let (epoch, version) = match reg.get_layer(&doc_id, &layer_id) {
                Some(l) => (l.epoch(), reg.get_history_version(&doc_id).unwrap_or(0)),
                None => (0, 0),
            };
            Ok(PixelHistoryResultJson {
                layer_id,
                tiles: Vec::new(),
                epoch,
                version,
            })
        }
    }
}

/// Redo: push the authoritative history stream forward, re-apply post-stroke
/// state, return `after` tiles + new epoch + new `DocumentVersion`. Symmetric to undo.
#[tauri::command]
pub fn rust_pixels_redo(
    doc_id: String,
    layer_id: String,
) -> Result<PixelHistoryResultJson, String> {
    let mut reg = registry();
    let reg = reg.get_or_insert_with(Default::default);
    match reg.redo_pixel(&doc_id) {
        Some((lid, tiles, epoch, version)) => Ok(PixelHistoryResultJson {
            layer_id: lid,
            tiles: tiles.iter().map(patch_to_wire).collect(),
            epoch,
            version,
        }),
        None => {
            let (epoch, version) = match reg.get_layer(&doc_id, &layer_id) {
                Some(l) => (l.epoch(), reg.get_history_version(&doc_id).unwrap_or(0)),
                None => (0, 0),
            };
            Ok(PixelHistoryResultJson {
                layer_id,
                tiles: Vec::new(),
                epoch,
                version,
            })
        }
    }
}

/// Phase 1: route a TS (non-pixel) logical mutation into the SAME unified
/// `ProtocolEngine` cursor so mixed TS/Rust operations share one history position.
/// Records an `External` entry (no pixel delta); advances the cursor + bumps
/// `DocumentVersion` exactly once. Returns the new epoch/version for TS cache sync.
/// Undo/redo runs through Rust's walker, which restores the captured order itself;
/// the host cannot replay its own snapshot over facade-owned layers.
#[tauri::command]
pub fn rust_pixels_record_external(
    doc_id: String,
    label: String,
    affected: Vec<String>,
    adapter_id: String,
    token: String,
    doc_size_before: Option<(f64, f64)>,
    doc_size_after: Option<(f64, f64)>,
    minted_layer_ids: Option<Vec<String>>,
) -> Result<PatchResultJson, String> {
    let mut reg = registry();
    let reg = reg.get_or_insert_with(Default::default);
    reg.record_external(
        &doc_id,
        &label,
        &affected,
        &adapter_id,
        &token,
        0,
        doc_size_before,
        doc_size_after,
        minted_layer_ids.as_deref().unwrap_or(&[]),
    )
    .map_err(|e| e)?;
    let version = reg.get_history_version(&doc_id).unwrap_or(0);
    let epoch = affected
        .first()
        .and_then(|l| reg.get_layer(&doc_id, l).map(|ly| ly.epoch()))
        .unwrap_or(0);
    Ok(PatchResultJson {
        tiles: Vec::new(),
        epoch,
        version,
    })
}

/// Read back a region of the canonical buffer (bounded; for verification/transport).
#[tauri::command]
pub fn rust_pixels_snapshot_tile(
    doc_id: String,
    layer_id: String,
    x: i64,
    y: i64,
    w: usize,
    h: usize,
) -> Result<TilePatchJson, String> {
    let reg = registry();
    let reg = reg
        .as_ref()
        .ok_or_else(|| "pixel store not initialized".to_string())?;
    let layer = reg
        .get_layer(&doc_id, &layer_id)
        .ok_or_else(|| format!("layer not initialized: {layer_id}"))?;
    let data = layer.snapshot_region(x, y, w, h);
    Ok(TilePatchJson {
        key: format!("{},{}", x / 256, y / 256),
        x,
        y,
        w,
        h,
        data_base64: b64(&data),
    })
}

/// C5.2 rehydration: return every tile of the layer as absolute TilePatches so
/// the TS derived cache can be rebuilt from the Rust canonical state. Only the
/// affected layer is shipped — never the whole document.
#[tauri::command]
pub fn rust_pixels_snapshot_layer(
    doc_id: String,
    layer_id: String,
) -> Result<Vec<TilePatchJson>, String> {
    let reg = registry();
    let reg = reg
        .as_ref()
        .ok_or_else(|| "pixel store not initialized".to_string())?;
    let layer = reg
        .get_layer(&doc_id, &layer_id)
        .ok_or_else(|| format!("layer not initialized: {layer_id}"))?;
    Ok(layer.all_tiles().iter().map(patch_to_wire).collect())
}

/// C5.2: current canonical epoch for the layer (cache-validity check).
#[tauri::command]
pub fn rust_pixels_get_epoch(doc_id: String, layer_id: String) -> Result<u64, String> {
    let reg = registry();
    let reg = reg
        .as_ref()
        .ok_or_else(|| "pixel store not initialized".to_string())?;
    reg.get_epoch(&doc_id, &layer_id)
}

/// C5.4 fill pilot: write an explicit RGBA region as one canonical `Pixel`
/// history entry. Returns the POST-image tile patches + new epoch/version so TS
/// can drive its derived cache (PaintTileSurface). Mirrors the brush commit's
/// single canonical step — no independent TS-visible step.
///
/// The reply carries NO pre-image (`WriteRegionJson`). Undo does not need one
/// from here: this write already appended the pre-image to Rust's pixel history,
/// and `rust_pixels_undo` returns it. A host that wanted the pre-image for its
/// own history memento was building it from tiles it already held.
#[tauri::command]
pub fn rust_pixels_write_region(
    doc_id: String,
    layer_id: String,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    rgba_base64: String,
) -> Result<WriteRegionJson, String> {
    if w < 0 || h < 0 {
        return Err(format!("negative region dimensions: {w}x{h}"));
    }
    let rgba = unb64(&rgba_base64, "rust_pixels_write_region rgbaBase64")?;
    let mut reg = registry();
    let reg = reg.get_or_insert_with(Default::default);
    let (_pre_image, after, epoch, version) = reg
        .write_region(&doc_id, &layer_id, x, y, w as usize, h as usize, rgba)
        .ok_or_else(|| format!("layer not initialized or region out of bounds: {layer_id}"))?;
    Ok(WriteRegionJson {
        after: after.iter().map(patch_to_wire).collect(),
        epoch,
        version,
    })
}

// Dev-only tip registry: TS-owned tip cache ships mask bytes ONCE per cache-miss
// (locked design decision); subsequent shadow calls reference by id. Keeps
// large-tip (3000px = 36MB) payloads off the JSON transport entirely.
static TIP_REGISTRY: Mutex<Option<HashMap<String, (usize, usize, Vec<u8>)>>> = Mutex::new(None);

#[tauri::command]
pub fn paint_parity_tip_register(tip_id: String, w: usize, h: usize, data: Vec<u8>) {
    let mut reg = TIP_REGISTRY.lock().unwrap();
    let map = reg.get_or_insert_with(HashMap::new);
    map.insert(tip_id, (w, h, data));
}

fn resolve_tip(req: &ParityShadowRequest) -> Result<(usize, usize, Vec<u8>), String> {
    if req.tip_data.is_empty() {
        if let Some(id) = &req.tip_id {
            let reg = TIP_REGISTRY.lock().unwrap();
            if let Some(map) = reg.as_ref() {
                if let Some((w, h, data)) = map.get(id) {
                    return Ok((*w, *h, data.clone()));
                }
            }
            return Err(format!("tip_id not registered: {id}"));
        }
        return Err("no tip_data and no tip_id".into());
    }
    Ok((req.tip_w, req.tip_h, req.tip_data.clone()))
}

#[derive(serde::Deserialize)]
pub struct ParityDabIn {
    pub x: f64,
    pub y: f64,
    pub alpha: f64,
}

#[derive(serde::Deserialize)]
pub struct ParityShadowRequest {
    pub w: usize,
    pub h: usize,
    pub prep_white: bool,
    pub eraser: bool,
    pub brush: f64,
    pub dabs: Vec<ParityDabIn>,
    pub tip_w: usize,
    pub tip_h: usize,
    /// Empty when referencing a registered tip via `tip_id`.
    #[serde(default)]
    pub tip_data: Vec<u8>,
    #[serde(default)]
    pub tip_id: Option<String>,
    /// C2: when true, Rust builds the tip itself via canonical_tip (TipSpec).
    #[serde(default)]
    pub canonical_tip: bool,
    /// Hardness for the canonical tip builder (required when canonical_tip).
    #[serde(default)]
    pub hardness: Option<f64>,
    /// Paint color for the canonical tip builder.
    #[serde(default)]
    pub tip_color: Option<[u8; 3]>,
}

#[derive(serde::Serialize)]
pub struct ParityShadowResponse {
    pub digest: String,
    pub tile_count: usize,
    pub changed_bytes: usize,
    pub prep_us: u128,
    pub raster_us: u128,
    pub patchgen_us: u128,
    /// Per-tile "(key,hash)" sorted pairs — JS diffs without shipping bytes.
    pub tile_hashes: Vec<(String, String)>,
    /// Only populated when `include_tiles` was requested (small cases / diagnostics).
    pub tiles: Vec<TilePatchJson>,
}

#[derive(serde::Deserialize)]
pub struct ParityShadowOpts {
    #[serde(default)]
    pub include_tiles: bool,
}

fn dabs_of(req: &ParityShadowRequest) -> Vec<ParityDab> {
    req.dabs
        .iter()
        .map(|d| ParityDab {
            x: d.x,
            y: d.y,
            alpha: d.alpha,
        })
        .collect()
}

fn tile_json(t: photrez_core::paint_parity::TilePatchOut) -> TilePatchJson {
    TilePatchJson {
        key: t.key,
        x: t.x,
        y: t.y,
        w: t.w,
        h: t.h,
        data_base64: b64(&t.data),
    }
}

fn effective_tip(req: &ParityShadowRequest) -> Result<(usize, usize, Vec<u8>), String> {
    if req.canonical_tip {
        let spec = TipSpec::new(
            req.brush,
            req.hardness.unwrap_or(0.8),
            req.tip_color.unwrap_or([225, 90, 23]),
        );
        let t = build_canonical_tip(&spec);
        // stamp bitmap is DIAMETER-space: width = height = diameter
        let w = t.diameter as usize;
        return Ok((w, w, t.rgba_diameter));
    }
    resolve_tip(req)
}

#[tauri::command]
pub fn paint_parity_shadow(
    req: ParityShadowRequest,
    opts: ParityShadowOpts,
) -> Result<ParityShadowResponse, String> {
    let (tw, th, tdata) = effective_tip(&req)?;
    let tip = ParityTip {
        width: tw,
        height: th,
        data: &tdata,
    };
    let dabs = dabs_of(&req);
    let (meta, tiles) = raster_shadow(
        req.w,
        req.h,
        req.prep_white,
        None,
        req.eraser,
        req.brush,
        &dabs,
        &tip,
        opts.include_tiles,
    );
    Ok(ParityShadowResponse {
        digest: meta.digest,
        tile_count: meta.tile_count,
        changed_bytes: meta.changed_bytes,
        prep_us: meta.prep_us,
        raster_us: meta.raster_us,
        patchgen_us: meta.patchgen_us,
        tile_hashes: meta.tile_hashes,
        tiles: tiles.into_iter().map(tile_json).collect(),
    })
}

/// Bug 1 + Bug 2 fix — canonical-owner brush commit.
/// Ensures the layer exists (idempotent: inits only when Rust has no store
/// entry), composites the dab batch onto the EXISTING canonical pixels, then
/// applies the result through the unified history. Returns the exact pre-stroke
/// (`before`) and composited (`after`) tiles + new epoch/version. Rust remains
/// the single canonical pixel owner; `after` is the FINAL tile state.
#[tauri::command]
pub fn paint_parity_commit(
    doc_id: String,
    layer_id: String,
    bytes: Vec<u8>,
    req: ParityShadowRequest,
    opts: ParityShadowOpts,
) -> Result<PaintCommitJson, String> {
    let (tw, th, tdata) = effective_tip(&req)?;
    let tip = ParityTip {
        width: tw,
        height: th,
        data: &tdata,
    };
    let dabs = dabs_of(&req);
    let mut reg = registry();
    let reg = reg.get_or_insert_with(Default::default);
    let (before, after, epoch, version) = reg
        .commit_pixels(
            &doc_id,
            &layer_id,
            bytes,
            req.w,
            req.h,
            req.eraser,
            req.brush,
            &dabs,
            &tip,
            opts.include_tiles,
        )
        .ok_or_else(|| format!("layer not initialized: {layer_id}"))?;
    Ok(PaintCommitJson {
        before: before.iter().map(patch_to_wire).collect(),
        after: after.iter().map(patch_to_wire).collect(),
        epoch,
        version,
    })
}

#[tauri::command]
pub fn paint_parity_tiles_for_keys(
    req: ParityShadowRequest,
    keys: Vec<String>,
) -> Result<Vec<TilePatchJson>, String> {
    let (tw, th, tdata) = effective_tip(&req)?;
    let tip = ParityTip {
        width: tw,
        height: th,
        data: &tdata,
    };
    let dabs = dabs_of(&req);
    Ok(tiles_for_keys(
        req.w,
        req.h,
        req.prep_white,
        req.eraser,
        req.brush,
        &dabs,
        &tip,
        &keys,
    )
    .into_iter()
    .map(tile_json)
    .collect())
}

/// Dev-only memory stats from the counting allocator (R1 harness).
#[derive(serde::Serialize)]
pub struct MemStats {
    pub current_mb: f64,
    pub peak_mb: f64,
}

#[tauri::command]
pub fn paint_parity_mem() -> MemStats {
    MemStats {
        current_mb: crate::alloc_stats::current_mb(),
        peak_mb: crate::alloc_stats::peak_mb(),
    }
}

#[tauri::command]
pub fn paint_parity_mem_reset() {
    crate::alloc_stats::reset_peak();
}

/// Dev-gate: parity harness auto-runs at app start only when this env is set
/// (set by the outer verification script; never set in production launches).
#[tauri::command]
pub fn paint_parity_autorun_enabled() -> bool {
    matches!(std::env::var("PHOTREZ_PARITY_AUTO"), Ok(ref v) if v == "1")
}

/// Dev-only result export: harness JSON lands in %TEMP% so the outer script can
/// collect it without any browser automation / CDP attachment.
#[tauri::command]
pub fn paint_parity_export(result_json: String) -> Result<String, String> {
    let path = std::env::temp_dir().join("photrez-parity-inshell.json");
    std::fs::write(&path, result_json.as_bytes()).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn paint_shadow_autorun_enabled() -> bool {
    matches!(std::env::var("PHOTREZ_SHADOW_AUTO"), Ok(ref v) if v == "1")
}

#[tauri::command]
pub fn paint_shadow_export(result_json: String) -> Result<String, String> {
    let path = std::env::temp_dir().join("photrez-shadow-inshell.json");
    std::fs::write(&path, result_json.as_bytes()).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

// Serializes ALL tests that touch the process-global `registry()` (both test
// both the Rust pixel-path tests and the document snapshot/restore tests). The parallel cargo harness lets one test's `reset()`
// (which sets the global to `None`) wipe another test's documents mid-run, so
// registry-touching tests must run one-at-a-time to be deterministic.
#[cfg(test)]
pub(crate) static TEST_REGISTRY_LOCK: Mutex<()> = Mutex::new(());

// The write reply's wire shape, split out because this file was already over the
// 1000-line guard.
#[cfg(test)]
mod write_region_reply_tests;

// ── C5.1 + C5.2 runtime integration test ────────────────────────────────────
// Exercises the EXACT command path the frontend invokes (open → init → patch →
// snapshot → undo → redo → snapshot_layer). This is the real backend code that
// runs in the app, so it is the empirical gate for "Rust owns the canonical
// buffer + undo/redo + rehydration" under document-namespaced ownership.
#[cfg(test)]
mod c4_runtime_tests {
    use super::*;

    fn wire(x: i64, y: i64, w: usize, h: usize, data: Vec<u8>) -> TilePatchWire {
        TilePatchWire {
            x,
            y,
            w,
            h,
            data_base64: b64(&data),
        }
    }

    /// Round-trip a serialized tile back to bytes the way the host does, so these
    /// assertions read pixels and never the encoded form.
    fn tile_bytes(t: &TilePatchJson) -> Vec<u8> {
        unb64(&t.data_base64, "tile dataBase64").expect("tile decodes")
    }

    fn reset() {
        *registry() = None;
    }

    #[test]
    fn c5_command_pipeline_canonical_undo_redo_and_epoch() {
        let _g = super::TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        let doc = "doc1".to_string();
        let layer = "c5rt".to_string();
        rust_pixels_open_document(doc.clone());
        let w = 512u32;
        let h = 512u32;
        let seed: Vec<u8> = vec![255; (w * h * 4) as usize];
        seed_layer_bytes(doc.clone(), layer.clone(), w, h, seed).unwrap();

        // Epoch starts at 0 (initial TS cache matches canonical).
        assert_eq!(
            rust_pixels_get_epoch(doc.clone(), layer.clone()).unwrap(),
            0
        );

        // One dirty tile at (0,0): 256x256 filled with 42.
        let before = vec![wire(0, 0, 256, 256, vec![255; 256 * 256 * 4])];
        let after = vec![wire(0, 0, 256, 256, vec![42; 256 * 256 * 4])];
        let res = apply_tile_patch(doc.clone(), layer.clone(), before, after).expect("patch");
        assert_eq!(res.tiles.len(), 1, "one delta entry returned");
        assert_eq!(res.epoch, 1, "epoch bumped after commit");
        assert_eq!(res.version, 1, "DocumentVersion bumped once after commit");

        // Canonical buffer is authoritative: snapshot reflects the after-state.
        let snap =
            rust_pixels_snapshot_tile(doc.clone(), layer.clone(), 0, 0, 256, 256).expect("snap");
        assert!(
            tile_bytes(&snap).iter().all(|&b| b == 42),
            "canonical = after stroke (42)"
        );

        // Undo → pre-stroke (255); epoch advances.
        let u = rust_pixels_undo(doc.clone(), layer.clone()).expect("undo");
        assert_eq!(u.tiles.len(), 1);
        assert_eq!(u.epoch, 2);
        assert_eq!(u.version, 2, "DocumentVersion bumps on undo");
        let snap2 =
            rust_pixels_snapshot_tile(doc.clone(), layer.clone(), 0, 0, 256, 256).expect("snap2");
        assert!(
            tile_bytes(&snap2).iter().all(|&b| b == 255),
            "canonical = pre-stroke after undo (255)"
        );

        // Redo → after-stroke (42); epoch advances again.
        let r = rust_pixels_redo(doc.clone(), layer.clone()).expect("redo");
        assert_eq!(r.tiles.len(), 1);
        assert_eq!(r.epoch, 3);
        assert_eq!(r.version, 3, "DocumentVersion bumps on redo");
        let snap3 =
            rust_pixels_snapshot_tile(doc.clone(), layer.clone(), 0, 0, 256, 256).expect("snap3");
        assert!(
            tile_bytes(&snap3).iter().all(|&b| b == 42),
            "canonical = post-stroke after redo (42)"
        );

        // Rehydration transport: snapshot_layer returns all tiles, bounded to the layer.
        let all = rust_pixels_snapshot_layer(doc.clone(), layer.clone()).expect("snapshot_layer");
        assert_eq!(all.len(), 4, "512x512 = 4 tiles of 256");
        let total: usize = all.iter().map(|t| t.w * t.h).sum();
        assert_eq!(total, 512 * 512);
        // A serialized tile round-trips: b64 -> bytes, every byte intact. This is
        // the Rust half of the cross-language decode contract the host's
        // decodePixelBytes has to match.
        for t in &all {
            let bytes = tile_bytes(t);
            assert_eq!(
                bytes.len(),
                t.w * t.h * 4,
                "tile {}x{} carries its whole RGBA payload",
                t.w,
                t.h
            );
        }

        // Bounded transport: snapshot of a sub-region only returns that region,
        // not the full 512x512 buffer.
        let sub = rust_pixels_snapshot_tile(doc.clone(), layer.clone(), 0, 0, 64, 64).expect("sub");
        assert_eq!(
            tile_bytes(&sub).len(),
            64 * 64 * 4,
            "snapshot is bounded to requested region"
        );
    }

    #[test]
    fn c5_command_empty_undo_is_safe() {
        let _g = super::TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        let doc = "doc1".to_string();
        let layer = "c5empty".to_string();
        rust_pixels_open_document(doc.clone());
        seed_layer_bytes(doc.clone(), layer.clone(), 64, 64, vec![0; 64 * 64 * 4]).unwrap();
        let u = rust_pixels_undo(doc.clone(), layer.clone()).expect("undo on empty");
        assert!(u.tiles.is_empty(), "undo on empty history returns no tiles");
    }

    #[test]
    fn c5_document_close_releases_storage() {
        let _g = super::TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        let doc = "docA".to_string();
        let layer = "L".to_string();
        rust_pixels_open_document(doc.clone());
        seed_layer_bytes(doc.clone(), layer.clone(), 64, 64, vec![0; 64 * 64 * 4]).unwrap();
        rust_pixels_close_document(doc.clone());
        let snap = rust_pixels_snapshot_tile(doc.clone(), layer.clone(), 0, 0, 64, 64);
        assert!(snap.is_err(), "closed document has no pixel storage");
    }

    /// WIRE CONTRACT: the tile shape the TypeScript host actually sends, accepted
    /// through the REAL Tauri command boundary.
    ///
    /// `CommandHistory.commit` hands a tile memento to `apply_tile_patch` as
    /// `TileUploadLike[]` = `{x, y, width, height, data}`
    /// (apps/desktop/src/renderer/types.ts), and every producer in
    /// `useBrushOverlay` builds it that way. `TilePatchWire` ALIASES
    /// `width`/`height` onto its `w`/`h`, so both spellings deserialize and the
    /// bridge's tile arm mints a real `Pixel` entry.
    ///
    /// This drives the actual invoke path - a mock app's `invoke_handler` and a
    /// real `InvokeRequest` - rather than a standalone `serde_json::from_value`,
    /// because the thing under test is "does the alias survive TAURI's argument
    /// deserialization". Tauri v2 implements `CommandItem`'s `Deserializer` by
    /// forwarding every method to `serde_json::Value`'s own deserializer over the
    /// value at the argument key (tauri-2.11.5 `src/ipc/command.rs:83-178`), so
    /// this asserts the same thing the app's IPC layer does.
    #[test]
    fn both_tile_shapes_are_accepted_at_the_wire_and_mint_exactly_one_entry() {
        let _g = super::TEST_REGISTRY_LOCK.lock().unwrap();
        reset();

        let app = tauri::test::mock_builder()
            .invoke_handler(tauri::generate_handler![
                rust_pixels_open_document,
                rust_pixels_init,
                apply_tile_patch,
                rust_pixels_record_external
            ])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app builds");
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("mock webview builds");

        let call = |cmd: &str, body: serde_json::Value| {
            tauri::test::get_ipc_response(
                &webview,
                tauri::webview::InvokeRequest {
                    cmd: cmd.into(),
                    callback: tauri::ipc::CallbackFn(0),
                    error: tauri::ipc::CallbackFn(1),
                    url: "http://tauri.localhost".parse().unwrap(),
                    body: tauri::ipc::InvokeBody::Json(body),
                    headers: Default::default(),
                    invoke_key: tauri::test::INVOKE_KEY.to_string(),
                },
            )
        };
        let ok = |r: Result<tauri::ipc::InvokeResponseBody, serde_json::Value>| {
            r.unwrap_or_else(|e| panic!("{e}"))
                .deserialize::<serde_json::Value>()
                .unwrap()
        };
        // The seed crosses as a base64 string - the transport the brush commit
        // uses. See rust_pixels_init.
        let seed_call = |doc: &str, bytes: Vec<u8>| {
            use base64::Engine;
            call(
                "rust_pixels_init",
                serde_json::json!({
                    "docId": doc,
                    "layerId": "lw",
                    "width": 64,
                    "height": 64,
                    "bytesBase64": base64::engine::general_purpose::STANDARD.encode(&bytes),
                }),
            )
        };

        let seed = vec![0u8; 64 * 64 * 4];
        let patch_args = |doc: &str, tiles: serde_json::Value| {
            serde_json::json!({
                "docId": doc, "layerId": "lw", "before": tiles, "after": tiles
            })
        };
        let payload = b64(&[1u8; 16]);
        let host_tiles =
            serde_json::json!([{ "x": 0, "y": 0, "width": 2, "height": 2, "dataBase64": payload }]);
        let rust_tiles =
            serde_json::json!([{ "x": 0, "y": 0, "w": 2, "h": 2, "dataBase64": payload }]);

        // THE HOST'S SHAPE. This is what every tile producer in the renderer sends.
        ok(call(
            "rust_pixels_open_document",
            serde_json::json!({ "docId": "host" }),
        ));
        ok(seed_call("host", seed.clone()));
        let from_host = ok(call("apply_tile_patch", patch_args("host", host_tiles)));
        assert_eq!(
            from_host["tiles"].as_array().unwrap().len(),
            1,
            "one delta tile returned"
        );
        assert_eq!(from_host["epoch"], 1, "epoch bumped once");
        assert_eq!(from_host["version"], 1, "DocumentVersion bumped once");
        assert_eq!(
            registry()
                .as_ref()
                .unwrap()
                .get_history_depth("host")
                .unwrap()
                .total_depth,
            1,
            "the host shape mints exactly one Pixel entry - the bridge's tile arm records"
        );

        // THE SHAPE THIS COMMAND DECLARES, on its own document. Same bytes, same
        // geometry: the two answers must be IDENTICAL, so the alias cannot have
        // dropped or defaulted a dimension on its way in.
        ok(call(
            "rust_pixels_open_document",
            serde_json::json!({ "docId": "rust" }),
        ));
        ok(seed_call("rust", seed.clone()));
        let from_rust = ok(call("apply_tile_patch", patch_args("rust", rust_tiles)));
        assert_eq!(
            from_host, from_rust,
            "the aliased spelling must be lossless: identical tiles, epoch and version"
        );
        assert_eq!(
            registry()
                .as_ref()
                .unwrap()
                .get_history_depth("rust")
                .unwrap()
                .total_depth,
            1,
            "the declared shape still records, so the alias did not replace it"
        );

        // NEUTRALITY CONTROL. The alias accepts a SECOND name for the dimension; it
        // must not have become a DEFAULT, which would turn a tile with no dimension
        // at all into a silently empty one. Both spellings absent is still rejected,
        // through this same real path - which is what makes the two accepts above
        // mean something.
        let rejected = call(
            "apply_tile_patch",
            patch_args(
                "rust",
                serde_json::json!([{ "x": 0, "y": 0, "data": vec![1u8; 16] }]),
            ),
        );
        let err = match rejected {
            Err(e) => e.to_string(),
            Ok(b) => panic!(
                "a tile with no dimension must still be rejected, got: {}",
                b.deserialize::<serde_json::Value>().unwrap()
            ),
        };
        assert!(
            err.contains("missing field"),
            "expected serde's missing-dimension rejection, got: {err}"
        );
        assert_eq!(
            registry()
                .as_ref()
                .unwrap()
                .get_history_depth("rust")
                .unwrap()
                .total_depth,
            1,
            "a rejected payload records nothing"
        );

        // NEUTRALITY CONTROL, part two: a tile carrying BOTH spellings must be REJECTED, not
        // silently resolved - serde's derive treats a field and its alias as one field
        // name, and `w` disagreeing with `width` is a silent wrong value on a pixel
        // dimension. Same real invoke path, so this is serde_derive's own answer.
        for (label, tile) in [
            (
                "agreeing",
                serde_json::json!({ "x": 0, "y": 0, "w": 2, "h": 2, "width": 2, "height": 2, "data": vec![1u8; 16] }),
            ),
            (
                "disagreeing",
                serde_json::json!({ "x": 0, "y": 0, "w": 2, "width": 64, "h": 2, "height": 64, "data": vec![1u8; 16] }),
            ),
        ] {
            let both = call(
                "apply_tile_patch",
                patch_args("rust", serde_json::json!([tile])),
            );
            match both {
                Err(e) => assert!(
                    e.to_string().contains("duplicate field"),
                    "serde must name this a duplicate field, not a missing one ({label}): {e}"
                ),
                Ok(b) => panic!(
                    "a tile carrying both spellings must be rejected, not resolved ({label}); got: {}",
                    b.deserialize::<serde_json::Value>().unwrap()
                ),
            }
            assert_eq!(
                registry()
                    .as_ref()
                    .unwrap()
                    .get_history_depth("rust")
                    .unwrap()
                    .total_depth,
                1,
                "a both-spellings payload records nothing ({label})"
            );
        }

        // POSITIVE CONTROL: the bridge's OTHER arm, all-scalar args, still records
        // the External entry a metadata host commit relies on.
        ok(call(
            "rust_pixels_open_document",
            serde_json::json!({ "docId": "meta" }),
        ));
        ok(call(
            "rust_pixels_record_external",
            serde_json::json!({
                "docId": "meta", "label": "Add Layer", "affected": [],
                "adapterId": "ts", "token": "Add Layer",
                "docSizeBefore": null, "docSizeAfter": null, "mintedLayerIds": null
            }),
        ));
        assert_eq!(
            registry()
                .as_ref()
                .unwrap()
                .get_history_depth("meta")
                .unwrap()
                .total_depth,
            1,
            "rust_pixels_record_external records an External entry"
        );
    }

    /// CROSS-LANGUAGE DECODE ROUND TRIP. The host's `decodePixelBytes` and this
    /// `unb64` are two implementations of one contract, and nothing else in the
    /// tree compares them: the host's own tests decode through the host's own
    /// encoder, which a symmetric pair of bugs would pass.
    ///
    /// So the expected string is captured from the RUST encoder here and pinned as
    /// a literal, and `decodePixelBytes` in
    /// apps/desktop/src/lib/protocol/pixelSeedCall.ts must produce these bytes
    /// from it (see pixelSeedCall.test.ts). Inputs chosen to hit the three cases a
    /// hand-rolled decoder gets wrong: lengths that straddle a base64 quantum, the
    /// `=` padding, and bytes that use every high bit.
    #[test]
    fn rust_encoder_output_decodes_to_the_original_bytes_on_the_host_side() {
        for len in [1usize, 2, 3, 4, 5, 6, 255, 256, 257] {
            let bytes: Vec<u8> = (0..len)
                .map(|i| (i as u8).wrapping_mul(37).wrapping_add(11))
                .collect();
            let encoded = b64(&bytes);
            let decoded = unb64(&encoded, "tile").expect("round trip decodes");
            assert_eq!(decoded, bytes, "len {len} survives b64 round trip");
            assert_eq!(encoded.len() % 4, 0, "base64 is always a multiple of 4");
            if len % 3 != 0 {
                assert!(encoded.ends_with('='), "len {len} pads to a quantum");
            }
        }
        // A pinned literal, so a change to the Rust alphabet or padding is caught
        // even if the round trip above still happened to be self-consistent.
        assert_eq!(b64(&[0u8, 255, 128, 64]), "AP+AQA==".to_string());
        assert_eq!(b64(&[1u8, 2, 3]), "AQID".to_string());
        assert_eq!(b64(&[1u8, 2]), "AQI=".to_string());
        assert_eq!(b64(&[1u8]), "AQ==".to_string());
    }

    /// The wire REJECTS a byte array where base64 is expected, rather than
    /// silently accepting one. That rejection is the mechanism: a caller that
    /// forgets to encode gets an error, not a store seeded with garbage.
    #[test]
    fn a_pixel_payload_that_is_not_base64_is_rejected_with_the_field_named() {
        let doc = "doc-b64".to_string();
        let layer = "L".to_string();
        rust_pixels_open_document(doc.clone());
        seed_layer_bytes(doc.clone(), layer.clone(), 4, 4, vec![0; 4 * 4 * 4]).unwrap();

        let err = rust_pixels_write_region(
            doc.clone(),
            layer.clone(),
            0,
            0,
            2,
            2,
            "[1,2,3,4]".to_string(),
        )
        .err()
        .expect("a JSON byte array is not base64");
        assert!(
            err.contains("rgbaBase64"),
            "the error names the field the caller got wrong: {err}"
        );

        let err = rust_pixels_resize_layer(
            doc.clone(),
            layer.clone(),
            4,
            4,
            "not base64 !!".to_string(),
        )
        .err()
        .expect("undecodable base64 is rejected");
        assert!(
            err.contains("bytesBase64"),
            "the error names the field the caller got wrong: {err}"
        );
        rust_pixels_close_document(doc);
    }
}
