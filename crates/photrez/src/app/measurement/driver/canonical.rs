//! The canonical pixel side of a run: the digest, the pixel diff and the
//! snapshot and commit validators that keep a sample honest.

use gpui_kit::{point, px, Bounds, Pixels};
use photrez_core::document::content::LayerType;

use crate::app::measurement::{
    InputMeasurementReceipt, InputMeasurementRender, InputMeasurementSnapshot, RefreshOutcome,
};
use crate::app::App;
use crate::display::canvas_bridge::fit_placement;

use super::CanonicalState;

pub(super) fn pixel_diff(
    before: &CanonicalState,
    after: &CanonicalState,
) -> Result<(usize, usize), String> {
    if before.width != after.width || before.height != after.height {
        return Err("canonical dimensions changed during a stroke".to_owned());
    }
    let width = before.width as usize;
    let height = before.height as usize;
    if width == 0 || height == 0 {
        return Err("canonical dimensions are zero".to_owned());
    }
    if before.pixels.len() != after.pixels.len() {
        return Err("canonical buffer length changed during a stroke".to_owned());
    }
    const TILE: usize = 256;
    let tiles_x = width.div_ceil(TILE);
    let tiles_y = height.div_ceil(TILE);
    let mut changed_tiles = vec![false; tiles_x.saturating_mul(tiles_y)];
    let mut changed_pixels = 0usize;
    for (index, (left, right)) in before
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(after.pixels.as_chunks::<4>().0.iter())
        .enumerate()
    {
        if left != right {
            changed_pixels = changed_pixels.saturating_add(1);
            let x = index % width;
            let y = index / width;
            let tile = (y / TILE) * tiles_x + (x / TILE);
            if let Some(flag) = changed_tiles.get_mut(tile) {
                *flag = true;
            }
        }
    }
    Ok((
        changed_pixels,
        changed_tiles.iter().filter(|&&flag| flag).count(),
    ))
}

pub(super) fn digest64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub(super) fn inspect_canonical(
    app: &mut App,
    cx: &mut gpui_kit::Context<App>,
) -> Result<CanonicalState, String> {
    let doc_id = app
        .active_document
        .as_ref()
        .ok_or_else(|| "no fixture document is active".to_owned())?
        .doc_id
        .clone();
    let session = app
        .sessions
        .get(&doc_id)
        .ok_or_else(|| "active document session is missing".to_owned())?;
    let loaded = session.loaded_document();
    let content = loaded
        .content_snapshot()
        .ok_or_else(|| "loaded document content is missing".to_owned())?;
    let layer_id = content
        .layers
        .iter()
        .find(|layer| matches!(layer.layer_type, LayerType::Raster))
        .map(|layer| layer.id.clone())
        .ok_or_else(|| "fixture has no raster layer".to_owned())?;
    let layer = loaded
        .registry
        .get_layer(&doc_id, &layer_id)
        .ok_or_else(|| "canonical raster pixels are missing".to_owned())?;
    let epoch = loaded
        .registry
        .get_epoch(&doc_id, &layer_id)
        .map_err(|error| format!("cannot read canonical epoch: {error}"))?;
    let version = loaded
        .registry
        .get_history_version(&doc_id)
        .ok_or_else(|| "canonical history version is missing".to_owned())?;
    Ok(CanonicalState {
        doc_id,
        layer_id,
        width: layer.width,
        height: layer.height,
        epoch,
        version,
        pixels: layer.pixels.clone(),
        foreground: app.state.read(cx).fg_color,
    })
}

pub(super) fn find_ink_target(state: &CanonicalState) -> Result<(u32, u32), String> {
    let expected = (state.width as usize)
        .checked_mul(state.height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "fixture pixel dimensions overflow".to_owned())?;
    if expected != state.pixels.len() {
        return Err("canonical pixel buffer has unexpected dimensions".into());
    }
    let index = state
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .position(|pixel| *pixel != state.foreground)
        .ok_or_else(|| "fixture has no pixel that differs from the Brush foreground".to_owned())?;
    Ok((
        (index % state.width as usize) as u32,
        (index / state.width as usize) as u32,
    ))
}

pub(super) fn nearby_pixel(start: (u32, u32), width: u32, height: u32) -> Option<(u32, u32)> {
    if start.0 + 1 < width {
        Some((start.0 + 1, start.1))
    } else if start.0 > 0 {
        Some((start.0 - 1, start.1))
    } else if start.1 + 1 < height {
        Some((start.0, start.1 + 1))
    } else if start.1 > 0 {
        Some((start.0, start.1 - 1))
    } else {
        None
    }
}

pub(super) fn point_for_pixel(
    bounds: Bounds<Pixels>,
    state: &CanonicalState,
    pixel: (u32, u32),
) -> Result<gpui_kit::Point<Pixels>, String> {
    let (page_x, page_y, page_width, page_height) = fit_placement(
        bounds.size.width.into(),
        bounds.size.height.into(),
        state.width as f32,
        state.height as f32,
    )
    .ok_or_else(|| "cannot fit fixture to production canvas".to_owned())?;
    Ok(bounds.origin
        + point(
            px(page_x + (pixel.0 as f32 + 0.5) / state.width as f32 * page_width),
            px(page_y + (pixel.1 as f32 + 0.5) / state.height as f32 * page_height),
        ))
}

pub(super) fn validate_snapshot(
    sample_id: u64,
    snapshot: &InputMeasurementSnapshot,
) -> Result<(), &'static str> {
    let receipt = snapshot.receipt.as_ref().ok_or("missing commit receipt")?;
    let render = snapshot
        .render
        .as_ref()
        .ok_or("missing App render marker")?;
    if receipt.sample_id != sample_id || render.sample_id != sample_id {
        return Err("stale receipt or render marker belongs to another sample");
    }
    if receipt.doc_id != render.doc_id
        || receipt.version != render.version
        || receipt.refresh != RefreshOutcome::Succeeded
        || receipt.t_down > receipt.t_up
        || receipt.t_up > receipt.t_commit
        || render.rendered_at < receipt.t_commit
    {
        return Err("receipt and render identities do not match a successful commit");
    }
    Ok(())
}

pub(super) fn validate_committed_state(
    sample_index: usize,
    before: &CanonicalState,
    after: &CanonicalState,
    receipt: &InputMeasurementReceipt,
    render: &InputMeasurementRender,
) -> Result<(), String> {
    if receipt.sample_id == 0
        || receipt.doc_id != before.doc_id
        || receipt.layer_id != before.layer_id
        || after.doc_id != before.doc_id
        || after.layer_id != before.layer_id
    {
        return Err(format!(
            "sample {sample_index}: document/layer identity changed"
        ));
    }
    if receipt.refresh != RefreshOutcome::Succeeded {
        return Err(format!("sample {sample_index}: document refresh failed"));
    }
    if receipt.epoch != after.epoch
        || receipt.version != after.version
        || receipt.epoch != before.epoch.saturating_add(1)
        || receipt.version != before.version.saturating_add(1)
    {
        return Err(format!(
            "sample {sample_index}: epoch/version receipt did not match the canonical commit"
        ));
    }
    if before.pixels == after.pixels {
        return Err(format!(
            "sample {sample_index}: Brush stroke did not change canonical pixels"
        ));
    }
    if receipt.t_down > receipt.t_up || receipt.t_up > receipt.t_commit {
        return Err(format!(
            "sample {sample_index}: receipt timestamps are out of order"
        ));
    }
    if let Some(preview) = receipt.t_preview {
        if preview < receipt.t_down || preview > receipt.t_up {
            return Err(format!(
                "sample {sample_index}: preview timestamp is out of order"
            ));
        }
    }
    if render.sample_id != receipt.sample_id
        || render.doc_id != receipt.doc_id
        || render.version != receipt.version
        || render.rendered_at < receipt.t_commit
    {
        return Err(format!(
            "sample {sample_index}: App render marker is stale or mismatched"
        ));
    }
    Ok(())
}
