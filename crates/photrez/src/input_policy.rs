//! # input_policy - pointer and brush chain
//!
//! Owns: pointer/brush input policy - turning window events into tool actions
//! and `photrez-core` commands.
//!
//! Must not own: painting (`display/`), widget layout (`views/`), or document
//! mutation outside core commands.

pub mod brush;

use std::rc::Rc;

use photrez_core::raster::brush::engine::BrushStrokeEngine;
use photrez_core::raster::brush::tip::{bilinear_resize_premul, BrushTip};
use photrez_core::raster::paint::raster::{raster_shadow, RasterDab, RasterTip};

use crate::state::ToolId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CursorPolicy {
    SystemDefault,
    Crosshair,
    Text,
    Grabbing,
    /// The clone affordance: the 0.1 build switches to `cursor: copy` while the
    /// pointer holds Alt during a Move drag.
    Cloning,
    OpenHand,
    ResizeNwse,
    ResizeNesw,
    ResizeEw,
    ResizeNs,
    NotAllowed,
}

/// The tool-level cursor inputs. Handle and rotate cursors are not decided here:
/// `App::canvas_cursor_policy` hit-tests the crop and transform handles and
/// replaces this value with the matching resize or crosshair variant.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CursorContext {
    pub tool: ToolId,
    pub phase: GesturePhase,
    pub layer_locked: bool,
    pub layer_paintable: bool,
    /// A Move transform is in flight on the canvas.
    pub dragging: bool,
    /// Alt as the pointer last reported it.
    pub cloning: bool,
}

pub(crate) fn cursor_policy_for_tool(tool: ToolId) -> CursorPolicy {
    match tool {
        // Move's reference cursor varies by handle; this hitbox has no handle-specific regions.
        ToolId::Move | ToolId::Brush | ToolId::Eraser => CursorPolicy::SystemDefault,
        ToolId::Selection
        | ToolId::Crop
        | ToolId::Eyedropper
        | ToolId::PaintBucket
        | ToolId::Gradient
        | ToolId::Shape => CursorPolicy::Crosshair,
        ToolId::Text => CursorPolicy::Text,
    }
}

pub(crate) fn cursor_policy_for_context(context: CursorContext) -> CursorPolicy {
    let writes = matches!(
        context.tool,
        ToolId::Brush | ToolId::Eraser | ToolId::PaintBucket | ToolId::Gradient
    );
    if writes && (!context.layer_paintable || context.layer_locked) {
        return CursorPolicy::NotAllowed;
    }
    if context.tool == ToolId::Move && (context.phase == GesturePhase::Action || context.dragging) {
        return if context.cloning {
            CursorPolicy::Cloning
        } else {
            CursorPolicy::Grabbing
        };
    }
    cursor_policy_for_tool(context.tool)
}

/// A non-brush pointer gesture that is waiting for pointer-up.
pub struct PendingGesture {
    pub tool: ToolId,
    pub start: (f64, f64),
    pub last: (f64, f64),
    /// Shift constrains a pixel selection to a square.
    pub shift: bool,
    /// Alt grows a pixel selection from its centre instead of its corner.
    pub alt: bool,
}

/// The explicit gesture lifecycle from the input contract.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GesturePhase {
    #[default]
    Idle,
    Action,
    Committed,
    Cleanup,
}

impl GesturePhase {
    fn can_advance_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Idle, Self::Action)
                | (Self::Action, Self::Committed)
                | (Self::Committed, Self::Cleanup)
                | (Self::Cleanup, Self::Idle)
        )
    }
}

/// The core layer a gesture mutates, captured at pointer-down.
pub struct PaintTarget {
    pub doc_id: String,
    pub layer_id: String,
    pub width: u32,
    pub height: u32,
    pub base_pixels: Vec<u8>,
}

/// The layer a brush/eraser stroke targets. It carries no full-layer pixel
/// copy; the preview reads the canonical crop and the commit reads the store.
pub struct BrushTarget {
    pub doc_id: String,
    pub layer_id: String,
    pub width: u32,
    pub height: u32,
    pub display_scale: f64,
}

/// One in-flight brush or eraser stroke.
pub struct BrushStroke {
    pub target: BrushTarget,
    pub engine: BrushStrokeEngine,
    pub dabs: Vec<RasterDab>,
    pub tip: Rc<BrushTip>,
    pub eraser: bool,
    pub size: f64,
    /// `opacity * flow` from the tool settings; every dab carries it.
    pub dab_alpha: f64,
    pub start: (f64, f64),
    pub last: (f64, f64),
}

/// One-entry tip cache keyed by the settings that shape the mask.
///
/// The paint overlay pre-warms this off the pointer path; building an 800 px
/// mask and its diagnostic upscales on every stroke cost ~40 ms of
/// first-pixel latency. A changed size/hardness/colour rebuilds once.
#[derive(Default)]
pub(crate) struct BrushTipCache {
    entry: Option<(u64, u64, [u8; 4], Rc<BrushTip>)>,
}

impl BrushTipCache {
    pub(crate) fn get(&mut self, size: f64, hardness: f64, color: [u8; 4]) -> Rc<BrushTip> {
        let key = (size.to_bits(), hardness.to_bits(), color);
        if let Some((size_bits, hardness_bits, cached_color, tip)) = &self.entry {
            if (*size_bits, *hardness_bits, *cached_color) == key {
                return Rc::clone(tip);
            }
        }
        let tip = Rc::new(brush::build_tip(size, hardness, color));
        self.entry = Some((key.0, key.1, key.2, Rc::clone(&tip)));
        tip
    }
}

#[derive(Default)]
pub struct InputState {
    phase: GesturePhase,
    brush: Option<BrushStroke>,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn phase(&self) -> GesturePhase {
        self.phase
    }

    pub fn has_active_brush(&self) -> bool {
        self.brush.is_some()
    }

    /// Begin a brush/eraser stroke. Refuses unless the machine is idle.
    pub fn begin_brush(&mut self, stroke: BrushStroke) -> bool {
        if self.brush.is_some() || !self.advance(GesturePhase::Action) {
            return false;
        }
        self.brush = Some(stroke);
        true
    }

    /// Feed one document-space pointer move. Returns false when idle.
    pub fn update_brush(&mut self, x: f64, y: f64) -> bool {
        if self.phase != GesturePhase::Action {
            return false;
        }
        let Some(stroke) = self.brush.as_mut() else {
            return false;
        };
        stroke.engine.update(x, y);
        let produced = stroke.engine.dabs().to_vec();
        let alpha = stroke.dab_alpha;
        for pair in produced.chunks_exact(2) {
            stroke.dabs.push(RasterDab {
                x: pair[0],
                y: pair[1],
                alpha,
            });
        }
        stroke.last = (x, y);
        true
    }

    /// Close the action phase and hand the stroke to the committer.
    pub fn finish_brush(&mut self) -> Option<BrushStroke> {
        if self.phase != GesturePhase::Action || !self.advance(GesturePhase::Committed) {
            return None;
        }
        self.brush.take()
    }

    /// `Committed -> Cleanup -> Idle`; refuses out-of-order cleanup.
    pub fn cleanup(&mut self) -> bool {
        if self.phase != GesturePhase::Committed {
            return false;
        }
        self.advance(GesturePhase::Cleanup) && self.advance(GesturePhase::Idle)
    }

    /// Drop any in-flight stroke without committing (pointer cancel / Escape).
    pub fn cancel(&mut self) {
        self.brush = None;
        self.phase = GesturePhase::Idle;
    }

    fn advance(&mut self, next: GesturePhase) -> bool {
        if !self.phase.can_advance_to(next) {
            return false;
        }
        self.phase = next;
        true
    }

    pub(crate) fn active_brush(&self) -> Option<&BrushStroke> {
        self.brush.as_ref()
    }
}

/// One display-scale preview patch. `doc_*` is the document-space destination;
/// `image_*` is the smaller preview raster; the display scales the image into
/// the destination.
#[derive(Clone, Debug)]
pub(crate) struct ScaledPreviewPatch {
    pub doc_x: u32,
    pub doc_y: u32,
    pub doc_width: u32,
    pub doc_height: u32,
    pub image_width: u32,
    pub image_height: u32,
    pub data: Vec<u8>,
}

/// Document-space rectangle a preview paints over.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PreviewRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Pixels a preview is rendered on top of: the whole composited document or a
/// crop read back from the display.
pub(crate) struct PreviewBase<'a> {
    pixels: &'a [u8],
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    doc_width: u32,
    doc_height: u32,
}

impl<'a> PreviewBase<'a> {
    /// Document-sized base pixels, used by the generator tests.
    #[cfg(test)]
    pub(crate) fn document(pixels: &'a [u8], width: u32, height: u32) -> Self {
        Self {
            pixels,
            x: 0,
            y: 0,
            width,
            height,
            doc_width: width,
            doc_height: height,
        }
    }

    /// Crop-sized base pixels read from the display composite. Rendering the
    /// stroke over the composite is what makes a preview correct on documents
    /// with more than one layer.
    pub(crate) fn region(
        pixels: &'a [u8],
        rect: PreviewRect,
        doc_width: u32,
        doc_height: u32,
    ) -> Self {
        Self {
            pixels,
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            doc_width,
            doc_height,
        }
    }
}

/// The document rectangle a stroke's dabs can touch, including the stamp
/// margin. `None` means the stroke has no dabs yet.
pub(crate) fn preview_rect(
    dabs: &[RasterDab],
    tip: &BrushTip,
    doc_width: u32,
    doc_height: u32,
) -> Result<Option<PreviewRect>, String> {
    if doc_width == 0 || doc_height == 0 {
        return Err("preview layer dimensions are zero".to_owned());
    }
    if dabs.is_empty() {
        return Ok(None);
    }
    let doc_w = i64::from(doc_width);
    let doc_h = i64::from(doc_height);
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for dab in dabs {
        if !dab.x.is_finite() || !dab.y.is_finite() || !dab.alpha.is_finite() {
            return Err("preview dab coordinates are not finite".to_owned());
        }
        min_x = min_x.min(dab.x);
        min_y = min_y.min(dab.y);
        max_x = max_x.max(dab.x);
        max_y = max_y.max(dab.y);
    }
    let tip_span = (tip.diameter as f64)
        .max(tip.width as f64)
        .max(tip.height as f64);
    let margin = (tip_span / 2.0).ceil() as i64 + 2;
    let x0 = ((min_x.floor() as i64) - margin).clamp(0, doc_w - 1);
    let y0 = ((min_y.floor() as i64) - margin).clamp(0, doc_h - 1);
    let x1 = ((max_x.ceil() as i64) + margin).clamp(x0 + 1, doc_w);
    let y1 = ((max_y.ceil() as i64) + margin).clamp(y0 + 1, doc_h);
    Ok(Some(PreviewRect {
        x: x0 as u32,
        y: y0 as u32,
        width: (x1 - x0) as u32,
        height: (y1 - y0) as u32,
    }))
}

/// Render the whole current stroke at display scale over the composite crop.
/// The preview stays display-sized (bounded by `max_edge`) while the
/// pointer-up commit remains the exact full-resolution authority.
///
/// The tip is resampled to the preview stamp size first, so the preview
/// rasterizer reads a stamp-sized bitmap instead of the full-resolution tip.
pub(crate) fn scaled_preview_patches(
    base: PreviewBase<'_>,
    dabs: &[RasterDab],
    tip: &BrushTip,
    eraser: bool,
    display_scale: f64,
    max_edge: u32,
) -> Result<Vec<ScaledPreviewPatch>, String> {
    if !display_scale.is_finite() || display_scale <= 0.0 {
        return Err("preview display scale is invalid".to_owned());
    }
    let expected = (base.width as usize)
        .checked_mul(base.height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "preview base size overflow".to_owned())?;
    if base.pixels.len() != expected {
        return Err("preview base buffer length mismatch".to_owned());
    }
    let Some(rect) = preview_rect(dabs, tip, base.doc_width, base.doc_height)? else {
        return Ok(Vec::new());
    };
    if rect.x < base.x
        || rect.y < base.y
        || rect.x + rect.width > base.x + base.width
        || rect.y + rect.height > base.y + base.height
    {
        return Err("preview base does not cover the stroke crop".to_owned());
    }
    let x0 = i64::from(rect.x);
    let y0 = i64::from(rect.y);
    let crop_w = rect.width as usize;
    let crop_h = rect.height as usize;
    let source_width = base.width as usize;
    let source_height = base.height as usize;
    let local_x0 = i64::from(rect.x - base.x);
    let local_y0 = i64::from(rect.y - base.y);

    let base_scale = display_scale.clamp(0.0, 1.0);
    let cap = max_edge.max(1) as f64;
    let scale = base_scale
        .min(cap / crop_w as f64)
        .min(cap / crop_h as f64)
        .clamp(1e-4, 1.0);
    let preview_w = ((crop_w as f64 * scale).ceil() as usize).max(1);
    let preview_h = ((crop_h as f64 * scale).ceil() as usize).max(1);
    let preview_base = downsample_premultiplied(
        base.pixels,
        source_width,
        source_height,
        local_x0,
        local_y0,
        crop_w,
        crop_h,
        preview_w,
        preview_h,
    )?;
    let shifted: Vec<RasterDab> = dabs
        .iter()
        .map(|dab| RasterDab {
            x: (dab.x - x0 as f64) * scale,
            y: (dab.y - y0 as f64) * scale,
            alpha: dab.alpha,
        })
        .collect();
    let preview_brush = (tip.diameter as f64 * scale).max(1.0);
    let preview_tip_edge = (preview_brush.ceil() as u32)
        .min(tip.width.min(tip.height))
        .max(1);
    let preview_tip_data = resize_tip_premultiplied(tip, preview_tip_edge);
    let raster_tip = RasterTip {
        width: preview_tip_edge as usize,
        height: preview_tip_edge as usize,
        data: &preview_tip_data,
    };
    let (_, patches) = raster_shadow(
        preview_w,
        preview_h,
        true,
        Some(preview_base),
        eraser,
        preview_brush,
        &shifted,
        &raster_tip,
        true,
    );

    let mut result = Vec::with_capacity(patches.len());
    for patch in patches {
        let doc_x = (x0 as f64 + patch.x as f64 / scale)
            .round()
            .clamp(0.0, base.doc_width as f64 - 1.0) as u32;
        let doc_y = (y0 as f64 + patch.y as f64 / scale)
            .round()
            .clamp(0.0, base.doc_height as f64 - 1.0) as u32;
        let doc_right = (x0 as f64 + (patch.x + patch.w as i64) as f64 / scale)
            .round()
            .clamp(doc_x as f64 + 1.0, base.doc_width as f64) as u32;
        let doc_bottom = (y0 as f64 + (patch.y + patch.h as i64) as f64 / scale)
            .round()
            .clamp(doc_y as f64 + 1.0, base.doc_height as f64) as u32;
        let image_width =
            u32::try_from(patch.w).map_err(|_| "preview image width overflow".to_owned())?;
        let image_height =
            u32::try_from(patch.h).map_err(|_| "preview image height overflow".to_owned())?;
        let expected = image_width as usize * image_height as usize * 4;
        if patch.data.len() != expected {
            return Err("preview patch buffer length mismatch".to_owned());
        }
        result.push(ScaledPreviewPatch {
            doc_x,
            doc_y,
            doc_width: doc_right - doc_x,
            doc_height: doc_bottom - doc_y,
            image_width,
            image_height,
            data: patch.data,
        });
    }
    Ok(result)
}

/// Downsample the colorized tip to the preview stamp resolution. The
/// resampler works on premultiplied pixels and emits straight RGBA, so the
/// result feeds `RasterTip` unchanged.
fn resize_tip_premultiplied(tip: &BrushTip, edge: u32) -> Vec<u8> {
    let mut premultiplied = tip.rgba_data_size.clone();
    for pixel in premultiplied.chunks_exact_mut(4) {
        let alpha = pixel[3] as u32;
        for channel in pixel.iter_mut().take(3) {
            *channel = ((*channel as u32 * alpha + 127) / 255) as u8;
        }
    }
    bilinear_resize_premul(&premultiplied, tip.width, tip.height, edge, edge)
}

#[allow(clippy::too_many_arguments)] // source/crop/destination are one geometric record
fn downsample_premultiplied(
    source: &[u8],
    source_width: usize,
    source_height: usize,
    x0: i64,
    y0: i64,
    crop_w: usize,
    crop_h: usize,
    out_w: usize,
    out_h: usize,
) -> Result<Vec<u8>, String> {
    if out_w == 0 || out_h == 0 || crop_w == 0 || crop_h == 0 {
        return Err("preview downsample dimensions are zero".to_owned());
    }
    let expected = source_width
        .checked_mul(source_height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "preview source size overflow".to_owned())?;
    if source.len() != expected {
        return Err("preview source buffer length mismatch".to_owned());
    }
    let x0 = usize::try_from(x0).map_err(|_| "preview source x is negative".to_owned())?;
    let y0 = usize::try_from(y0).map_err(|_| "preview source y is negative".to_owned())?;
    if x0.saturating_add(crop_w) > source_width || y0.saturating_add(crop_h) > source_height {
        return Err("preview source crop is outside the layer".to_owned());
    }

    let mut out = Vec::with_capacity(out_w * out_h * 4);
    for oy in 0..out_h {
        let sy0 = (oy * crop_h) / out_h;
        let sy1 = (((oy + 1) * crop_h) / out_h).max(sy0 + 1).min(crop_h);
        for ox in 0..out_w {
            let sx0 = (ox * crop_w) / out_w;
            let sx1 = (((ox + 1) * crop_w) / out_w).max(sx0 + 1).min(crop_w);
            let mut red = 0u64;
            let mut green = 0u64;
            let mut blue = 0u64;
            let mut alpha = 0u64;
            let mut count = 0u64;
            for sy in sy0..sy1 {
                let row = (y0 + sy) * source_width;
                for sx in sx0..sx1 {
                    let index = (row + x0 + sx) * 4;
                    let a = source[index + 3] as u64;
                    if a != 0 {
                        let half = a / 2;
                        red += (source[index] as u64 * a + half) / 255;
                        green += (source[index + 1] as u64 * a + half) / 255;
                        blue += (source[index + 2] as u64 * a + half) / 255;
                        alpha += a;
                    }
                    count += 1;
                }
            }
            out.extend_from_slice(&[
                (red.checked_div(count).unwrap_or(0)) as u8,
                (green.checked_div(count).unwrap_or(0)) as u8,
                (blue.checked_div(count).unwrap_or(0)) as u8,
                (alpha.checked_div(count).unwrap_or(0)) as u8,
            ]);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
