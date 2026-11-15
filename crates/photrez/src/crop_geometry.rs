//! Crop frame geometry: interaction modes, handle resizing, and the region
//! Apply sends to the core.

use crate::crop_aspect::CropMode;

/// The smallest width or height the crop frame may reach.
pub(crate) const MIN_CROP_SIZE: f64 = 1.0;
pub(crate) const MODERN_MIN_CROP_SCREEN_SIZE: f64 = 24.0;
pub(crate) const MODERN_DRAG_CREATE_MIN_SIZE: f64 = 100.0;

/// The largest output edge the core crop command accepts.
#[allow(dead_code)]
pub(crate) const MAX_CROP_OUTPUT: u32 = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum CropHandle {
    Nw,
    Ne,
    Sw,
    Se,
    N,
    S,
    E,
    W,
}

#[allow(dead_code)]
impl CropHandle {
    pub(crate) fn has_east(self) -> bool {
        matches!(self, Self::E | Self::Ne | Self::Se)
    }

    pub(crate) fn has_west(self) -> bool {
        matches!(self, Self::Nw | Self::Sw | Self::W)
    }

    pub(crate) fn has_north(self) -> bool {
        matches!(self, Self::Nw | Self::Ne | Self::N)
    }

    pub(crate) fn has_south(self) -> bool {
        matches!(self, Self::Sw | Self::Se | Self::S)
    }

    pub(crate) fn is_corner(self) -> bool {
        matches!(self, Self::Nw | Self::Ne | Self::Sw | Self::Se)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CropInteraction {
    #[default]
    Modern,
    Classic,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CropResizeOptions {
    pub mode: CropMode,
    pub ratio: Option<f64>,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CropGuides {
    None,
    #[default]
    Thirds,
    Grid,
    Diagonal,
    Golden,
}

impl CropGuides {
    pub(crate) const ALL: [Self; 5] = [
        Self::None,
        Self::Thirds,
        Self::Grid,
        Self::Diagonal,
        Self::Golden,
    ];
}

#[derive(Clone, Debug)]
pub(crate) struct CropSnapTargets {
    pub(crate) x: Vec<f64>,
    pub(crate) y: Vec<f64>,
}

pub(crate) fn crop_guide_segments(
    width: f64,
    height: f64,
    guides: CropGuides,
) -> Vec<([f64; 2], [f64; 2])> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }

    let mut segments = Vec::new();
    match guides {
        CropGuides::None => {}
        CropGuides::Thirds | CropGuides::Golden => {
            let positions = if guides == CropGuides::Thirds {
                [1.0 / 3.0, 2.0 / 3.0]
            } else {
                [0.382, 0.618]
            };
            for position in positions {
                let x = width * position;
                let y = height * position;
                segments.push(([x, 0.0], [x, height]));
                segments.push(([0.0, y], [width, y]));
            }
        }
        CropGuides::Grid => {
            let divisions = ((width * height).sqrt() / 64.0).ceil().max(1.0);
            // Cap at 4096 divisions; raise only if supported viewports exceed 262k px.
            if !divisions.is_finite() || divisions > 4096.0 {
                return Vec::new();
            }
            let divisions = divisions as usize;
            for index in 1..divisions {
                let position = index as f64 / divisions as f64;
                let x = width * position;
                let y = height * position;
                segments.push(([x, 0.0], [x, height]));
                segments.push(([0.0, y], [width, y]));
            }
        }
        CropGuides::Diagonal => {
            segments.push(([0.0, 0.0], [width, height]));
            segments.push(([width, 0.0], [0.0, height]));
        }
    }
    segments
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CropField {
    Size,
    Custom,
    Angle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropSnapshot {
    pub frame: [f64; 4],
    pub offset: (f64, f64),
    pub rotation: f64,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropViewport {
    pub pan_x: f64,
    pub pan_y: f64,
    pub zoom: f64,
}

pub(crate) struct ModernCropCreateInput {
    pub start: (f64, f64),
    pub end: (f64, f64),
    pub viewport: (f64, f64),
    pub camera: CropViewport,
    pub mode: CropMode,
    pub aspect: Option<(u32, u32)>,
    pub size_target: Option<(u32, u32)>,
    pub shift: bool,
    pub rotation: f64,
}

/// The rotation degrees the core command applies for Modern interaction.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn modern_apply_rotation(rotation: f64) -> f64 {
    if rotation == 0.0 { 0.0 } else { -rotation }
}

pub(crate) fn inverse_rotate_delta(delta: (f64, f64), rotation: f64) -> (f64, f64) {
    let (sin, cos) = (-rotation).to_radians().sin_cos();
    (delta.0 * cos - delta.1 * sin, delta.0 * sin + delta.1 * cos)
}

/// The crop region in the original document for Apply and keyboard commit.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn crop_apply_region(
    interaction: CropInteraction,
    frame: [f64; 4],
    offset: (f64, f64),
    viewport: CropViewport,
) -> [f64; 4] {
    match interaction {
        CropInteraction::Classic => frame,
        CropInteraction::Modern => {
            let zoom = viewport.zoom.max(0.01);
            let [_, _, w, h] = frame;
            let center_x = (frame[0] + w / 2.0) * zoom + viewport.pan_x;
            let center_y = (frame[1] + h / 2.0) * zoom + viewport.pan_y;
            let pivot_x = (center_x - viewport.pan_x - offset.0) / zoom;
            let pivot_y = (center_y - viewport.pan_y - offset.1) / zoom;
            [pivot_x - w / 2.0, pivot_y - h / 2.0, w, h]
        }
    }
}

/// The largest frame with the current aspect that fits the viewport and document.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn default_modern_crop_frame(
    viewport: (f64, f64),
    document: (f64, f64),
    zoom: f64,
    pan: (f64, f64),
    aspect: Option<(u32, u32)>,
) -> [f64; 4] {
    let (viewport_w, viewport_h) = viewport;
    let (doc_w, doc_h) = document;
    let (pan_x, pan_y) = pan;
    let zoom = zoom.max(0.01);
    let visible_w = viewport_w / zoom;
    let visible_h = viewport_h / zoom;
    let max_w = visible_w.min(doc_w);
    let max_h = visible_h.min(doc_h);
    let aspect_ratio = aspect
        .filter(|(w, h)| *w > 0 && *h > 0)
        .map(|(w, h)| f64::from(w) / f64::from(h))
        .unwrap_or_else(|| max_w / max_h);

    let mut w = max_w;
    let mut h = w / aspect_ratio;
    if h > max_h {
        h = max_h;
        w = h * aspect_ratio;
    }

    let doc_center_x = (viewport_w / 2.0 - pan_x) / zoom;
    let doc_center_y = (viewport_h / 2.0 - pan_y) / zoom;
    let w = w.round().max(1.0);
    let h = h.round().max(1.0);
    let x = (doc_center_x - w / 2.0).round();
    let y = (doc_center_y - h / 2.0).round();
    [x, y, w, h]
}

pub(crate) fn modern_crop_create(input: ModernCropCreateInput) -> CropSnapshot {
    let selection_width = (input.end.0 - input.start.0).abs();
    let selection_height = (input.end.1 - input.start.1).abs();
    let (width, height) = match input.mode {
        CropMode::Free if input.shift => {
            let size = selection_width.max(selection_height);
            (size, size)
        }
        CropMode::Free => (selection_width, selection_height),
        CropMode::Ratio
            if let Some((aspect_width, aspect_height)) = input
                .aspect
                .filter(|(width, height)| *width > 0 && *height > 0) =>
        {
            let aspect = f64::from(aspect_width) / f64::from(aspect_height);
            let area = (selection_width * selection_height)
                .max(MODERN_DRAG_CREATE_MIN_SIZE * MODERN_DRAG_CREATE_MIN_SIZE);
            let width = (area * aspect).sqrt();
            (width, width / aspect)
        }
        CropMode::Size
            if let Some((width, height)) = input
                .size_target
                .filter(|(width, height)| *width > 0 && *height > 0) =>
        {
            (f64::from(width), f64::from(height))
        }
        CropMode::Ratio | CropMode::Size => (selection_width, selection_height),
    };
    let width = width.max(MODERN_DRAG_CREATE_MIN_SIZE);
    let height = height.max(MODERN_DRAG_CREATE_MIN_SIZE);
    let center_x = (input.viewport.0 / 2.0 - input.camera.pan_x) / input.camera.zoom;
    let center_y = (input.viewport.1 / 2.0 - input.camera.pan_y) / input.camera.zoom;
    let selection_center_x = (input.start.0 + input.end.0) / 2.0;
    let selection_center_y = (input.start.1 + input.end.1) / 2.0;
    CropSnapshot {
        frame: [
            (center_x - width / 2.0 + 0.5).floor(),
            (center_y - height / 2.0 + 0.5).floor(),
            width,
            height,
        ],
        offset: (
            input.viewport.0 / 2.0 - selection_center_x * input.camera.zoom - input.camera.pan_x,
            input.viewport.1 / 2.0 - selection_center_y * input.camera.zoom - input.camera.pan_y,
        ),
        rotation: input.rotation,
    }
}

pub(crate) fn crop_snap_targets(
    doc_width: f64,
    doc_height: f64,
    layer_bounds: impl IntoIterator<Item = (f64, f64, f64, f64)>,
) -> CropSnapTargets {
    let mut x = Vec::new();
    let mut y = Vec::new();
    for target in [0.0, doc_width, doc_width / 2.0] {
        push_crop_snap_target(&mut x, target);
    }
    for target in [0.0, doc_height, doc_height / 2.0] {
        push_crop_snap_target(&mut y, target);
    }
    for (left, top, width, height) in layer_bounds {
        for target in [left, left + width, left + width / 2.0] {
            push_crop_snap_target(&mut x, target);
        }
        for target in [top, top + height, top + height / 2.0] {
            push_crop_snap_target(&mut y, target);
        }
    }
    for target in [doc_width / 3.0, doc_width * 2.0 / 3.0] {
        push_crop_snap_target(&mut x, target);
    }
    for target in [doc_height / 3.0, doc_height * 2.0 / 3.0] {
        push_crop_snap_target(&mut y, target);
    }
    CropSnapTargets { x, y }
}

fn push_crop_snap_target(targets: &mut Vec<f64>, target: f64) {
    if !targets.contains(&target) {
        targets.push(target);
    }
}

fn snap_crop_axis(
    frame: &mut [f64; 4],
    targets: &[f64],
    threshold: f64,
    vertical: bool,
    allowed_edges: [bool; 3],
    move_frame: bool,
) -> Option<(bool, f64)> {
    let (origin, extent) = if vertical { (0, 2) } else { (1, 3) };
    let edges = [
        frame[origin],
        frame[origin] + frame[extent],
        frame[origin] + frame[extent] / 2.0,
    ];
    let mut best_distance = threshold + 1.0;
    let mut best = None;
    for (edge, value) in edges.into_iter().enumerate() {
        if !allowed_edges[edge] {
            continue;
        }
        for target in targets {
            let distance = (value - target).abs();
            if distance < best_distance {
                best_distance = distance;
                best = Some((edge, *target));
            }
        }
    }
    let (edge, target) = best?;
    match (edge, move_frame) {
        (0, true) => frame[origin] = target,
        (1, true) => frame[origin] = target - frame[extent],
        (0, false) => {
            frame[extent] += frame[origin] - target;
            frame[origin] = target;
        }
        (1, false) => frame[extent] = target - frame[origin],
        (2, _) => frame[origin] = target - frame[extent] / 2.0,
        _ => return None,
    }
    Some((vertical, target))
}

pub(crate) fn snap_crop_creation(
    mut frame: [f64; 4],
    targets: &CropSnapTargets,
    threshold: f64,
) -> ([f64; 4], Vec<(bool, f64)>) {
    let mut guides = Vec::with_capacity(2);
    if let Some(guide) = snap_crop_axis(&mut frame, &targets.x, threshold, true, [true; 3], false) {
        guides.push(guide);
    }
    if let Some(guide) = snap_crop_axis(&mut frame, &targets.y, threshold, false, [true; 3], false)
    {
        guides.push(guide);
    }
    (frame, guides)
}

pub(crate) fn snap_classic_crop(
    mut frame: [f64; 4],
    handle: Option<CropHandle>,
    targets: &CropSnapTargets,
    threshold: f64,
) -> ([f64; 4], Vec<(bool, f64)>) {
    let (x_edges, y_edges) = match handle {
        None => ([true; 3], [true; 3]),
        Some(CropHandle::Nw) => ([true, false, false], [true, false, false]),
        Some(CropHandle::Ne) => ([false, true, false], [true, false, false]),
        Some(CropHandle::Sw) => ([true, false, false], [false, true, false]),
        Some(CropHandle::Se) => ([false, true, false], [false, true, false]),
        Some(CropHandle::N) => ([false, false, true], [true, false, false]),
        Some(CropHandle::S) => ([false, false, true], [false, true, false]),
        Some(CropHandle::E) => ([false, true, false], [false, false, true]),
        Some(CropHandle::W) => ([true, false, false], [false, false, true]),
    };
    let move_frame = handle.is_none();
    let mut guides = Vec::with_capacity(2);
    if let Some(guide) =
        snap_crop_axis(&mut frame, &targets.x, threshold, true, x_edges, move_frame)
    {
        guides.push(guide);
    }
    if let Some(guide) = snap_crop_axis(
        &mut frame, &targets.y, threshold, false, y_edges, move_frame,
    ) {
        guides.push(guide);
    }
    if frame[2] < MIN_CROP_SIZE {
        frame[2] = MIN_CROP_SIZE;
    }
    if frame[3] < MIN_CROP_SIZE {
        frame[3] = MIN_CROP_SIZE;
    }
    (frame, guides)
}

/// The crop values one Apply sends to the core crop command.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct CropRequest {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: Option<f64>,
    pub target: Option<(f64, f64)>,
    pub delete_pixels: bool,
    pub fill_background: Option<[u8; 4]>,
}

/// The crop region in the original document.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn effective_rect(
    interaction: CropInteraction,
    frame: [f64; 4],
    offset: (f64, f64),
) -> [f64; 4] {
    match interaction {
        CropInteraction::Modern => [frame[0] - offset.0, frame[1] - offset.1, frame[2], frame[3]],
        CropInteraction::Classic => frame,
    }
}

/// The ratio the current crop mode locks, or `None` when the frame is free.
pub(crate) fn locked_ratio(
    mode: CropMode,
    aspect: Option<(u32, u32)>,
    size_target: Option<(u32, u32)>,
) -> Option<f64> {
    let (w, h) = match mode {
        CropMode::Free => return None,
        CropMode::Ratio => aspect?,
        CropMode::Size => size_target?,
    };
    if w == 0 || h == 0 {
        return None;
    }
    Some(f64::from(w) / f64::from(h))
}

/// Resize the frame for Modern interaction.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn modern_resize(
    frame: [f64; 4],
    handle: CropHandle,
    delta: (f64, f64),
    options: CropResizeOptions,
    min_size: f64,
) -> ([f64; 4], (f64, f64)) {
    let (dx, dy) = delta;
    let CropResizeOptions {
        mode,
        ratio,
        shift,
        alt,
    } = options;
    let [x, y, w, h] = frame;
    let ratio = ratio.filter(|ratio| ratio.is_finite() && *ratio > 0.0);
    let (new_w, new_h) = if shift && handle.is_corner() {
        let dx = dx * if alt { 2.0 } else { 1.0 };
        let dy = dy * if alt { 2.0 } else { 1.0 };
        if handle.is_corner() && mode == CropMode::Free {
            let horizontal = if handle.has_east() { 1.0 } else { -1.0 };
            let vertical = if handle.has_south() { 1.0 } else { -1.0 };
            let factor = (1.0 / w)
                .max(1.0 / h)
                .max(1.0 + (dx * horizontal + dy * vertical) / (w + h));
            ((w * factor).max(min_size), (h * factor).max(min_size))
        } else {
            (
                (w + if handle.has_east() {
                    dx
                } else if handle.has_west() {
                    -dx
                } else {
                    0.0
                })
                .max(min_size),
                (h + if handle.has_south() {
                    dy
                } else if handle.has_north() {
                    -dy
                } else {
                    0.0
                })
                .max(min_size),
            )
        }
    } else {
        let dx = dx * 2.0;
        let dy = dy * 2.0;
        let grow_x = if handle.has_east() {
            dx
        } else if handle.has_west() {
            -dx
        } else {
            0.0
        };
        let grow_y = if handle.has_south() {
            dy
        } else if handle.has_north() {
            -dy
        } else {
            0.0
        };
        match ratio.filter(|_| !shift) {
            Some(ratio) if handle.is_corner() => {
                let horizontal = if handle.has_east() { 1.0 } else { -1.0 };
                let vertical = if handle.has_south() { 1.0 } else { -1.0 };
                let min_factor = (1.0 / w).max(ratio / w);
                let factor = min_factor.max(1.0 + (dx * horizontal + dy * vertical) / (w + h));
                let new_w = (w * factor).max(min_size);
                (new_w, new_w / ratio)
            }
            Some(ratio) if handle.has_north() || handle.has_south() => {
                let new_h = (h + grow_y).max(min_size);
                (new_h * ratio, new_h)
            }
            Some(ratio) => {
                let new_w = (w + grow_x).max(min_size);
                (new_w, new_w / ratio)
            }
            None => ((w + grow_x).max(min_size), (h + grow_y).max(min_size)),
        }
    };
    let new_frame = [x + (w - new_w) / 2.0, y + (h - new_h) / 2.0, new_w, new_h];
    let moved_x = new_w - w;
    let moved_y = new_h - h;
    let compensation = (
        if alt {
            0.0
        } else if handle.has_west() {
            moved_x / 2.0
        } else if handle.has_east() {
            -moved_x / 2.0
        } else {
            0.0
        },
        if alt {
            0.0
        } else if handle.has_north() {
            moved_y / 2.0
        } else if handle.has_south() {
            -moved_y / 2.0
        } else {
            0.0
        },
    );
    (new_frame, compensation)
}

/// Resize the frame for Classic interaction.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn classic_resize(
    frame: [f64; 4],
    handle: CropHandle,
    delta: (f64, f64),
    options: CropResizeOptions,
) -> [f64; 4] {
    let (dx, dy) = delta;
    let CropResizeOptions {
        mode,
        ratio,
        shift,
        alt,
    } = options;
    let [x, y, w, h] = frame;
    let dx = dx * if alt { 2.0 } else { 1.0 };
    let dy = dy * if alt { 2.0 } else { 1.0 };
    let aspect = (mode == CropMode::Ratio).then_some(ratio).flatten();
    let mut resized = if handle.is_corner() {
        match (mode, aspect, shift) {
            (CropMode::Free, _, true) => {
                let horizontal = if handle.has_east() { 1.0 } else { -1.0 };
                let vertical = if handle.has_south() { 1.0 } else { -1.0 };
                let factor = (1.0 / w)
                    .max(1.0 / h)
                    .max(1.0 + (dx * horizontal + dy * vertical) / (w + h));
                let new_w = (w * factor).max(MIN_CROP_SIZE);
                let new_h = (h * factor).max(MIN_CROP_SIZE);
                [
                    if handle.has_west() { x + w - new_w } else { x },
                    if handle.has_north() { y + h - new_h } else { y },
                    new_w,
                    new_h,
                ]
            }
            (CropMode::Ratio, Some(ratio), false) => {
                let horizontal = if handle.has_east() { 1.0 } else { -1.0 };
                let vertical = if handle.has_south() { 1.0 } else { -1.0 };
                let min_factor = (1.0 / w).max(ratio / w);
                let factor = min_factor.max(1.0 + (dx * horizontal + dy * vertical) / (w + h));
                let new_w = (w * factor).max(MIN_CROP_SIZE);
                let new_h = (new_w / ratio).max(MIN_CROP_SIZE);
                [
                    if handle.has_west() { x + w - new_w } else { x },
                    if handle.has_north() { y + h - new_h } else { y },
                    new_w,
                    new_h,
                ]
            }
            _ => {
                let mut left = x;
                let mut top = y;
                let mut width = w;
                let mut height = h;
                if handle.has_east() {
                    width += dx;
                }
                if handle.has_west() {
                    width -= dx;
                    left += dx;
                }
                if handle.has_south() {
                    height += dy;
                }
                if handle.has_north() {
                    height -= dy;
                    top += dy;
                }
                [
                    left,
                    top,
                    width.max(MIN_CROP_SIZE),
                    height.max(MIN_CROP_SIZE),
                ]
            }
        }
    } else if mode == CropMode::Ratio {
        if let Some(ratio) = aspect {
            if handle.has_north() || handle.has_south() {
                let new_h = (h + if handle.has_south() { dy } else { -dy }).max(MIN_CROP_SIZE);
                let new_w = new_h * ratio;
                [
                    x + (w - new_w) / 2.0,
                    if handle.has_south() { y } else { y + h - new_h },
                    new_w,
                    new_h,
                ]
            } else {
                let new_w = (w + if handle.has_east() { dx } else { -dx }).max(MIN_CROP_SIZE);
                let new_h = new_w / ratio;
                [
                    if handle.has_west() { x + w - new_w } else { x },
                    y + (h - new_h) / 2.0,
                    new_w,
                    new_h,
                ]
            }
        } else {
            classic_free_resize(frame, handle, dx, dy)
        }
    } else {
        classic_free_resize(frame, handle, dx, dy)
    };
    if alt {
        resized[0] = x + (w - resized[2]) / 2.0;
        resized[1] = y + (h - resized[3]) / 2.0;
    }
    resized
}

fn classic_free_resize(frame: [f64; 4], handle: CropHandle, dx: f64, dy: f64) -> [f64; 4] {
    let [x, y, w, h] = frame;
    let mut left = x;
    let mut top = y;
    let mut width = w;
    let mut height = h;
    if handle.has_east() {
        width += dx;
    }
    if handle.has_west() {
        width -= dx;
        left += dx;
    }
    if handle.has_south() {
        height += dy;
    }
    if handle.has_north() {
        height -= dy;
        top += dy;
    }
    [
        left,
        top,
        width.max(MIN_CROP_SIZE),
        height.max(MIN_CROP_SIZE),
    ]
}

pub(crate) fn drag_create(start: (f64, f64), end: (f64, f64), ratio: Option<f64>) -> [f64; 4] {
    let x = start.0.min(end.0);
    let y = start.1.min(end.1);
    let width = (end.0 - start.0).abs().max(MIN_CROP_SIZE);
    let height = (end.1 - start.1).abs().max(MIN_CROP_SIZE);
    match ratio {
        Some(ratio) => constrain_aspect([x, y, width, height], ratio),
        None => [x, y, width, height],
    }
}

pub(crate) fn constrain_aspect(frame: [f64; 4], ratio: f64) -> [f64; 4] {
    let [x, y, w, h] = frame;
    if w <= 0.0 || h <= 0.0 || ratio <= 0.0 {
        return frame;
    }
    if w / h > ratio {
        let width = h * ratio;
        [x + (w - width) / 2.0, y, width, h]
    } else {
        let height = w / ratio;
        [x, y + (h - height) / 2.0, w, height]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_snap_targets_deduplicate_without_changing_order() {
        let targets = crop_snap_targets(90.0, 60.0, [(10.0, 5.0, 20.0, 10.0)]);
        assert_eq!(targets.x, [0.0, 90.0, 45.0, 10.0, 30.0, 20.0, 60.0]);
        assert_eq!(targets.y, [0.0, 60.0, 30.0, 5.0, 15.0, 10.0, 20.0, 40.0]);

        let zero_size = crop_snap_targets(0.0, 0.0, []);
        assert_eq!(zero_size.x, [0.0]);
        assert_eq!(zero_size.y, [0.0]);
    }

    #[test]
    fn crop_creation_snaps_nearest_edges_and_keeps_first_ties() {
        let targets = CropSnapTargets {
            x: vec![0.0, 20.0],
            y: vec![0.0, 20.0],
        };
        let (frame, guides) = snap_crop_creation([6.0, 6.0, 8.0, 8.0], &targets, 6.0);
        assert_eq!(frame, [0.0, 0.0, 14.0, 14.0]);
        assert_eq!(guides, [(true, 0.0), (false, 0.0)]);
    }

    #[test]
    fn crop_snap_threshold_and_target_ties_match_the_oracle() {
        let targets = CropSnapTargets {
            x: vec![0.0],
            y: vec![],
        };
        let (within_oracle_cutoff, guides) =
            snap_crop_creation([6.5, 10.0, 8.0, 8.0], &targets, 6.0);
        assert_eq!(within_oracle_cutoff, [0.0, 10.0, 14.5, 8.0]);
        assert_eq!(guides, [(true, 0.0)]);

        let (outside_oracle_cutoff, guides) =
            snap_crop_creation([7.0, 10.0, 8.0, 8.0], &targets, 6.0);
        assert_eq!(outside_oracle_cutoff, [7.0, 10.0, 8.0, 8.0]);
        assert!(guides.is_empty());

        let targets = CropSnapTargets {
            x: vec![90.0, 110.0],
            y: vec![],
        };
        let (tied, guides) = snap_classic_crop(
            [95.0, 50.0, 10.0, 20.0],
            Some(CropHandle::N),
            &targets,
            10.0,
        );
        assert_eq!(tied, [85.0, 50.0, 10.0, 20.0]);
        assert_eq!(guides, [(true, 90.0)]);
    }

    #[test]
    fn classic_crop_move_keeps_size_and_side_resize_snaps_the_cross_axis_center() {
        let targets = CropSnapTargets {
            x: vec![200.0],
            y: vec![150.0],
        };
        let (moved, move_guides) =
            snap_classic_crop([198.0, 142.0, 20.0, 20.0], None, &targets, 2.0);
        assert_eq!(moved, [200.0, 140.0, 20.0, 20.0]);
        assert_eq!(move_guides, [(true, 200.0), (false, 150.0)]);

        let (resized, resize_guides) = snap_classic_crop(
            [100.0, 142.0, 98.0, 20.0],
            Some(CropHandle::E),
            &targets,
            2.0,
        );
        assert_eq!(resized, [100.0, 140.0, 100.0, 20.0]);
        assert_eq!(resize_guides, [(true, 200.0), (false, 150.0)]);
    }

    #[test]
    fn classic_resize_and_snap_keep_a_positive_minimum_without_clamping_origin() {
        let resized = classic_resize(
            [10.0, 20.0, 8.0, 6.0],
            CropHandle::Nw,
            (20.0, 30.0),
            CropResizeOptions {
                mode: CropMode::Free,
                ratio: None,
                shift: false,
                alt: false,
            },
        );
        assert_eq!(resized, [30.0, 50.0, 1.0, 1.0]);

        let targets = CropSnapTargets {
            x: vec![21.0],
            y: vec![],
        };
        let (snapped, guides) = snap_classic_crop(
            [10.0, 20.0, 10.0, 15.0],
            Some(CropHandle::W),
            &targets,
            12.0,
        );
        assert_eq!(snapped, [21.0, 20.0, 1.0, 15.0]);
        assert_eq!(guides, [(true, 21.0)]);
    }

    #[test]
    fn the_modern_region_is_the_frame_moved_back_by_the_picture_offset() {
        let frame = [10.0, 20.0, 100.0, 50.0];
        assert_eq!(
            effective_rect(CropInteraction::Modern, frame, (0.0, 0.0)),
            frame
        );
        assert_eq!(
            effective_rect(CropInteraction::Modern, frame, (4.0, -6.0)),
            [6.0, 26.0, 100.0, 50.0]
        );
        assert_eq!(
            effective_rect(CropInteraction::Classic, frame, (4.0, -6.0)),
            frame
        );
    }

    #[test]
    fn crop_guide_segments_match_the_selected_composition() {
        let thirds = crop_guide_segments(300.0, 180.0, CropGuides::Thirds);
        assert_eq!(thirds.len(), 4);
        assert_eq!(thirds[0], ([100.0, 0.0], [100.0, 180.0]));
        assert_eq!(thirds[1], ([0.0, 60.0], [300.0, 60.0]));

        let golden = crop_guide_segments(100.0, 80.0, CropGuides::Golden);
        assert_eq!(golden.len(), 4);
        assert!((golden[0].0[0] - 38.2).abs() < 1e-9);
        assert!((golden[1].0[1] - 30.56).abs() < 1e-9);

        assert_eq!(
            crop_guide_segments(128.0, 64.0, CropGuides::Grid),
            vec![([64.0, 0.0], [64.0, 64.0]), ([0.0, 32.0], [128.0, 32.0]),]
        );
        assert!(crop_guide_segments(64.0, 64.0, CropGuides::Grid).is_empty());
        assert_eq!(crop_guide_segments(128.0, 128.0, CropGuides::Grid).len(), 2);
        assert_eq!(
            crop_guide_segments(128.0, 128.0, CropGuides::Diagonal),
            vec![([0.0, 0.0], [128.0, 128.0]), ([128.0, 0.0], [0.0, 128.0]),]
        );
        assert!(crop_guide_segments(128.0, 128.0, CropGuides::None).is_empty());
    }

    #[test]
    fn crop_guide_segments_reject_invalid_and_unbounded_dimensions() {
        for (width, height) in [
            (0.0, 32.0),
            (32.0, -1.0),
            (f64::NAN, 32.0),
            (32.0, f64::INFINITY),
            (f64::MAX, f64::MAX),
        ] {
            assert!(
                crop_guide_segments(width, height, CropGuides::Grid).is_empty(),
                "frame={width}x{height}"
            );
        }
        assert_eq!(
            crop_guide_segments(262_144.0, 262_144.0, CropGuides::Grid).len(),
            8190
        );
        assert!(crop_guide_segments(262_208.0, 262_208.0, CropGuides::Grid).is_empty());
    }

    #[test]
    fn crop_apply_region_modern_uses_the_pivot_at_zoom_one() {
        let frame = [10.0, 20.0, 100.0, 50.0];
        let viewport = CropViewport {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
        };
        assert_eq!(
            crop_apply_region(CropInteraction::Modern, frame, (0.0, 0.0), viewport),
            frame
        );
        assert_eq!(
            crop_apply_region(CropInteraction::Modern, frame, (10.0, 0.0), viewport),
            [0.0, 20.0, 100.0, 50.0]
        );
        assert_eq!(
            crop_apply_region(CropInteraction::Classic, frame, (10.0, 0.0), viewport),
            frame
        );
    }

    #[test]
    fn default_modern_crop_frame_fits_landscape_document() {
        let frame =
            default_modern_crop_frame((800.0, 600.0), (1200.0, 800.0), 1.0, (200.0, 100.0), None);
        assert!(frame[2] <= 800.0 && frame[3] <= 600.0);
        assert!(frame[2] >= 1.0 && frame[3] >= 1.0);
    }

    #[test]
    fn max_crop_output_matches_core_limit() {
        assert_eq!(MAX_CROP_OUTPUT, 16_384);
    }
    #[test]
    fn modern_apply_rotation_negates_non_zero_preview() {
        assert_eq!(modern_apply_rotation(0.0), 0.0);
        assert_eq!(modern_apply_rotation(12.5), -12.5);
    }

    #[test]
    fn modern_offset_delta_inverse_rotates_screen_motion() {
        assert_eq!(inverse_rotate_delta((4.0, 0.0), 0.0), (4.0, 0.0));
        let rotated = inverse_rotate_delta((0.0, 4.0), 90.0);
        assert_eq!(rotated.0, 4.0);
        assert!(rotated.1.abs() < 1e-10);
    }

    #[test]
    fn modern_shift_edge_resize_keeps_the_double_delta() {
        let (frame, compensation) = modern_resize(
            [10.0, 20.0, 100.0, 50.0],
            CropHandle::E,
            (5.0, 0.0),
            CropResizeOptions {
                mode: CropMode::Free,
                ratio: None,
                shift: true,
                alt: false,
            },
            1.0,
        );
        assert_eq!(frame, [5.0, 20.0, 110.0, 50.0]);
        assert_eq!(compensation, (-5.0, 0.0));
    }

    #[test]
    fn modern_shift_edge_resize_ignores_locks_across_modes_and_alt() {
        let start = [10.0, 20.0, 100.0, 50.0];
        let cases = [
            (CropHandle::E, [5.0, 20.0, 110.0, 50.0], (-5.0, 0.0)),
            (CropHandle::W, [15.0, 20.0, 90.0, 50.0], (-5.0, 0.0)),
            (CropHandle::N, [10.0, 27.0, 100.0, 36.0], (0.0, -7.0)),
            (CropHandle::S, [10.0, 13.0, 100.0, 64.0], (0.0, -7.0)),
        ];
        for mode in [CropMode::Free, CropMode::Ratio, CropMode::Size] {
            for (handle, expected_frame, expected_compensation) in cases {
                for alt in [false, true] {
                    let (frame, compensation) = modern_resize(
                        start,
                        handle,
                        (5.0, 7.0),
                        CropResizeOptions {
                            mode,
                            ratio: Some(2.0),
                            shift: true,
                            alt,
                        },
                        1.0,
                    );
                    assert_eq!(frame, expected_frame, "{mode:?} {handle:?} alt={alt}");
                    assert_eq!(
                        compensation,
                        if alt {
                            (0.0, 0.0)
                        } else {
                            expected_compensation
                        },
                        "{mode:?} {handle:?} alt={alt}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_locked_ratio_follows_the_crop_mode() {
        assert_eq!(
            locked_ratio(CropMode::Free, Some((1, 1)), Some((800, 600))),
            None
        );
        assert_eq!(
            locked_ratio(CropMode::Ratio, Some((16, 9)), None),
            Some(16.0 / 9.0)
        );
        assert_eq!(
            locked_ratio(CropMode::Size, None, Some((800, 600))),
            Some(800.0 / 600.0)
        );
        assert_eq!(locked_ratio(CropMode::Ratio, None, Some((800, 600))), None);
    }
}
