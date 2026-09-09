// R1 SHADOW Tauri commands (dev parity harness only — NOT production pixel ownership).
// Naming deliberately avoids resource_read_* / TileStore per design constraints.

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

#[derive(serde::Deserialize)]
pub struct TilePatchWire {
    pub x: i64,
    pub y: i64,
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
}

#[derive(serde::Serialize)]
pub struct TilePatchJson {
    pub key: String,
    pub x: i64,
    pub y: i64,
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
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

fn patch_to_wire(t: &TilePatch) -> TilePatchJson {
    TilePatchJson {
        key: format!("{},{}", t.x / 256, t.y / 256),
        x: t.x,
        y: t.y,
        w: t.w,
        h: t.h,
        data: t.data.clone(),
    }
}

fn wire_to_patch(t: TilePatchWire) -> TilePatch {
    TilePatch {
        x: t.x,
        y: t.y,
        w: t.w,
        h: t.h,
        data: t.data,
    }
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
#[tauri::command]
pub fn rust_pixels_init(
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
    bytes: Vec<u8>,
) -> Result<(), String> {
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
    let b: Vec<TilePatch> = before.into_iter().map(wire_to_patch).collect();
    let a: Vec<TilePatch> = after.into_iter().map(wire_to_patch).collect();
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
/// The actual metadata revert on undo/redo stays TS-side; this entry only keeps
/// the ordering unified with Rust pixel operations.
#[tauri::command]
pub fn rust_pixels_record_external(
    doc_id: String,
    label: String,
    affected: Vec<String>,
    adapter_id: String,
    token: String,
) -> Result<PatchResultJson, String> {
    let mut reg = registry();
    let reg = reg.get_or_insert_with(Default::default);
    reg.record_external(&doc_id, &label, &affected, &adapter_id, &token, 0)
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
        data,
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
/// history entry. Returns the exact `before`/`after` tile patches + new
/// epoch/version so TS can drive its derived cache (PaintTileSurface) and its
/// history memento (undo/redo replays the before/after tiles). Mirrors the
/// brush commit's single canonical step — no independent TS-visible step.
#[tauri::command]
pub fn rust_pixels_write_region(
    doc_id: String,
    layer_id: String,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    rgba: Vec<u8>,
) -> Result<PaintCommitJson, String> {
    if w < 0 || h < 0 {
        return Err(format!("negative region dimensions: {w}x{h}"));
    }
    let mut reg = registry();
    let reg = reg.get_or_insert_with(Default::default);
    let (before, after, epoch, version) = reg
        .write_region(&doc_id, &layer_id, x, y, w as usize, h as usize, rgba)
        .ok_or_else(|| format!("layer not initialized or region out of bounds: {layer_id}"))?;
    Ok(PaintCommitJson {
        before: before.iter().map(patch_to_wire).collect(),
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
        data: t.data,
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

// ── C5.1 + C5.2 runtime integration test ────────────────────────────────────
// Exercises the EXACT command path the frontend invokes (open → init → patch →
// snapshot → undo → redo → snapshot_layer). This is the real backend code that
// runs in the app, so it is the empirical gate for "Rust owns the canonical
// buffer + undo/redo + rehydration" under document-namespaced ownership.
#[cfg(test)]
mod c4_runtime_tests {
    use super::*;

    fn wire(x: i64, y: i64, w: usize, h: usize, data: Vec<u8>) -> TilePatchWire {
        TilePatchWire { x, y, w, h, data }
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
        rust_pixels_init(doc.clone(), layer.clone(), w, h, seed).unwrap();

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
            snap.data.iter().all(|&b| b == 42),
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
            snap2.data.iter().all(|&b| b == 255),
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
            snap3.data.iter().all(|&b| b == 42),
            "canonical = post-stroke after redo (42)"
        );

        // Rehydration transport: snapshot_layer returns all tiles, bounded to the layer.
        let all = rust_pixels_snapshot_layer(doc.clone(), layer.clone()).expect("snapshot_layer");
        assert_eq!(all.len(), 4, "512x512 = 4 tiles of 256");
        let total: usize = all.iter().map(|t| t.w * t.h).sum();
        assert_eq!(total, 512 * 512);

        // Bounded transport: snapshot of a sub-region only returns that region,
        // not the full 512x512 buffer.
        let sub = rust_pixels_snapshot_tile(doc.clone(), layer.clone(), 0, 0, 64, 64).expect("sub");
        assert_eq!(
            sub.data.len(),
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
        rust_pixels_init(doc.clone(), layer.clone(), 64, 64, vec![0; 64 * 64 * 4]).unwrap();
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
        rust_pixels_init(doc.clone(), layer.clone(), 64, 64, vec![0; 64 * 64 * 4]).unwrap();
        rust_pixels_close_document(doc.clone());
        let snap = rust_pixels_snapshot_tile(doc.clone(), layer.clone(), 0, 0, 64, 64);
        assert!(snap.is_err(), "closed document has no pixel storage");
    }
}
