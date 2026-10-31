//! Port of the frozen `viewport/transformGeometry.ts`.
//!
//! All coordinates are document space unless named `screen`. Angles are
//! degrees, positive = clockwise in screen space (Y-down), exactly the frozen
//! convention.

use photrez_core::ptz::content::{LayerType, PtzLayer, Transform2D};

pub(crate) const DEG: f64 = std::f64::consts::PI / 180.0;
pub(crate) const HANDLE_HIT_PX: f64 = 16.0;
pub(crate) const ROTATE_THRESHOLD_PX: f64 = 250.0;

/// The size the selection frame uses for a layer.
///
/// A shape layer's bitmap is document-sized while the drawn rectangle lives in
/// `shape_params`, so the frame (corners, handles, resize math, rotation
/// centre) must come from the shape rectangle. Raster, text and group layers
/// carry their frame size in `width`/`height`.
pub(crate) fn layer_frame_size(layer: &PtzLayer) -> (f64, f64) {
    match layer.layer_type {
        LayerType::Shape => layer
            .shape_params
            .as_ref()
            .map(|params| (params.width, params.height))
            .unwrap_or((layer.width, layer.height)),
        _ => (layer.width, layer.height),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Handle {
    Nw,
    Ne,
    Se,
    Sw,
    N,
    E,
    S,
    W,
    Move,
    Rotate,
}

impl Handle {
    pub(crate) fn contains_east(self) -> bool {
        matches!(self, Handle::Ne | Handle::Se | Handle::E)
    }
    pub(crate) fn contains_west(self) -> bool {
        matches!(self, Handle::Nw | Handle::Sw | Handle::W)
    }
    pub(crate) fn contains_north(self) -> bool {
        matches!(self, Handle::Nw | Handle::Ne | Handle::N)
    }
    pub(crate) fn contains_south(self) -> bool {
        matches!(self, Handle::Sw | Handle::Se | Handle::S)
    }
    pub(crate) fn is_corner(self) -> bool {
        matches!(self, Handle::Nw | Handle::Ne | Handle::Se | Handle::Sw)
    }
}

pub(crate) fn rotate_point(point: Point, center: Point, deg: f64) -> Point {
    let rad = deg * DEG;
    let (cos, sin) = (rad.cos(), rad.sin());
    let dx = point.x - center.x;
    let dy = point.y - center.y;
    Point {
        x: center.x + dx * cos - dy * sin,
        y: center.y + dx * sin + dy * cos,
    }
}

pub(crate) fn layer_center(transform: &Transform2D, width: f64, height: f64) -> Point {
    let effective_w = width * transform.scale_x.abs();
    let effective_h = height * transform.scale_y.abs();
    Point {
        x: transform.x + effective_w / 2.0,
        y: transform.y + effective_h / 2.0,
    }
}

/// Visual corners in TL, TR, BR, BL order. Flip mirrors the texture only, so
/// the visual rectangle ignores `flip_h`/`flip_v`.
pub(crate) fn layer_corners(transform: &Transform2D, width: f64, height: f64) -> [Point; 4] {
    let effective_w = width * transform.scale_x.abs();
    let effective_h = height * transform.scale_y.abs();
    let raw = [
        Point {
            x: transform.x,
            y: transform.y,
        },
        Point {
            x: transform.x + effective_w,
            y: transform.y,
        },
        Point {
            x: transform.x + effective_w,
            y: transform.y + effective_h,
        },
        Point {
            x: transform.x,
            y: transform.y + effective_h,
        },
    ];
    if transform.rotation == 0.0 {
        return raw;
    }
    let center = layer_center(transform, width, height);
    raw.map(|corner| rotate_point(corner, center, transform.rotation))
}

#[allow(dead_code)]
pub(crate) fn layer_aabb(transform: &Transform2D, width: f64, height: f64) -> (f64, f64, f64, f64) {
    let corners = layer_corners(transform, width, height);
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for corner in corners {
        min_x = min_x.min(corner.x);
        min_y = min_y.min(corner.y);
        max_x = max_x.max(corner.x);
        max_y = max_y.max(corner.y);
    }
    (min_x, min_y, max_x - min_x, max_y - min_y)
}

#[allow(dead_code)]
pub(crate) fn point_to_layer_local(
    point: Point,
    transform: &Transform2D,
    width: f64,
    height: f64,
) -> Point {
    let center = layer_center(transform, width, height);
    rotate_point(point, center, -transform.rotation)
}

pub(crate) fn document_to_layer_local(
    doc_x: f64,
    doc_y: f64,
    transform: &Transform2D,
    width: f64,
    height: f64,
) -> Point {
    let center = layer_center(transform, width, height);
    let rotated = rotate_point(
        Point {
            x: doc_x - center.x,
            y: doc_y - center.y,
        },
        Point { x: 0.0, y: 0.0 },
        -transform.rotation,
    );
    let flip_x = if transform.flip_h { -1.0 } else { 1.0 };
    let flip_y = if transform.flip_v { -1.0 } else { 1.0 };
    let sx = transform.scale_x * flip_x;
    let sy = transform.scale_y * flip_y;
    Point {
        x: (if sx != 0.0 { rotated.x / sx } else { 0.0 }) + width / 2.0,
        y: (if sy != 0.0 { rotated.y / sy } else { 0.0 }) + height / 2.0,
    }
}

#[allow(dead_code)]
pub(crate) fn nearest_rotate_corner(
    point: Point,
    transform: &Transform2D,
    width: f64,
    height: f64,
) -> usize {
    let center = layer_center(transform, width, height);
    let local = rotate_point(point, center, -transform.rotation);
    let effective_w = width * transform.scale_x.abs();
    let effective_h = height * transform.scale_y.abs();
    let cx = transform.x + effective_w / 2.0;
    let cy = transform.y + effective_h / 2.0;
    if local.x <= cx && local.y <= cy {
        0
    } else if local.x > cx && local.y <= cy {
        1
    } else if local.x > cx && local.y > cy {
        2
    } else {
        3
    }
}

/// Handle hit test in document space. `zoom` maps the frozen 16 px/250 px
/// screen tolerances into document units.
pub(crate) fn detect_handle(
    point: Point,
    transform: &Transform2D,
    width: f64,
    height: f64,
    zoom: f64,
) -> Option<Handle> {
    detect_handle_with_rotate(point, transform, width, height, zoom, ROTATE_THRESHOLD_PX)
}

/// Press hit test. The oracle's interactive rotate region is the adaptive
/// 32..60 px ring around the frame (`rotateBand.ts:getAdaptiveRotateBandPx`),
/// while the 250 px threshold only drives the hover cursor. A press outside
/// that ring belongs to the active tool instead (the Shape tool draws again).
pub(crate) fn detect_press_handle(
    point: Point,
    transform: &Transform2D,
    width: f64,
    height: f64,
    zoom: f64,
) -> Option<Handle> {
    let screen_w = width * transform.scale_x.abs() * zoom;
    let screen_h = height * transform.scale_y.abs() * zoom;
    detect_handle_with_rotate(
        point,
        transform,
        width,
        height,
        zoom,
        adaptive_rotate_band_px(screen_w, screen_h),
    )
}

pub(crate) fn adaptive_rotate_band_px(width: f64, height: f64) -> f64 {
    (width.abs().min(height.abs()) * 0.15 + 20.0)
        .round()
        .clamp(32.0, 60.0)
}

fn detect_handle_with_rotate(
    point: Point,
    transform: &Transform2D,
    width: f64,
    height: f64,
    zoom: f64,
    rotate_screen_px: f64,
) -> Option<Handle> {
    let center = layer_center(transform, width, height);
    let local = rotate_point(point, center, -transform.rotation);
    let effective_w = width * transform.scale_x.abs();
    let effective_h = height * transform.scale_y.abs();
    let rect_x = transform.x;
    let rect_y = transform.y;
    let handle_hit = HANDLE_HIT_PX / zoom.max(f64::EPSILON);
    let rotate_threshold = rotate_screen_px / zoom.max(f64::EPSILON);

    let corners = [
        (Handle::Nw, rect_x, rect_y),
        (Handle::Ne, rect_x + effective_w, rect_y),
        (Handle::Se, rect_x + effective_w, rect_y + effective_h),
        (Handle::Sw, rect_x, rect_y + effective_h),
    ];
    for (id, x, y) in corners {
        if ((local.x - x).powi(2) + (local.y - y).powi(2)).sqrt() <= handle_hit {
            return Some(id);
        }
    }
    let sides = [
        (Handle::N, rect_x + effective_w / 2.0, rect_y),
        (Handle::E, rect_x + effective_w, rect_y + effective_h / 2.0),
        (Handle::S, rect_x + effective_w / 2.0, rect_y + effective_h),
        (Handle::W, rect_x, rect_y + effective_h / 2.0),
    ];
    for (id, x, y) in sides {
        if ((local.x - x).powi(2) + (local.y - y).powi(2)).sqrt() <= handle_hit {
            return Some(id);
        }
    }
    let inside = local.x >= rect_x
        && local.x <= rect_x + effective_w
        && local.y >= rect_y
        && local.y <= rect_y + effective_h;
    if inside {
        return Some(Handle::Move);
    }
    let outside = local.x < rect_x
        || local.x > rect_x + effective_w
        || local.y < rect_y
        || local.y > rect_y + effective_h;
    if outside {
        let expanded_left = rect_x - rotate_threshold;
        let expanded_top = rect_y - rotate_threshold;
        let expanded_w = effective_w + rotate_threshold * 2.0;
        let expanded_h = effective_h + rotate_threshold * 2.0;
        if local.x >= expanded_left
            && local.x <= expanded_left + expanded_w
            && local.y >= expanded_top
            && local.y <= expanded_top + expanded_h
        {
            return Some(Handle::Rotate);
        }
    }
    None
}

/// Exact port of `applyResizeHandle`. `shift_key` is the frozen "break aspect
/// ratio" arg; `alt_key` resizes around the centre.
#[allow(clippy::too_many_arguments)] // the frozen handle record is positional; a struct would only rename it
pub(crate) fn apply_resize_handle(
    transform: &Transform2D,
    width: f64,
    height: f64,
    handle: Handle,
    screen_dx: f64,
    screen_dy: f64,
    shift_key: bool,
    alt_key: bool,
) -> Transform2D {
    let rad = -transform.rotation * DEG;
    let (cos, sin) = (rad.cos(), rad.sin());
    let dx = screen_dx * cos - screen_dy * sin;
    let dy = screen_dx * sin + screen_dy * cos;

    let abs_sx = transform.scale_x.abs();
    let abs_sy = transform.scale_y.abs();
    let mut visual_w = width * abs_sx;
    let mut visual_h = height * abs_sy;
    let mut visual_x = transform.x;
    let mut visual_y = transform.y;
    let local_dx = if alt_key { dx * 2.0 } else { dx };
    let local_dy = if alt_key { dy * 2.0 } else { dy };
    let old_w = visual_w;
    let old_h = visual_h;
    let corner = handle.is_corner();
    let keep_aspect = !shift_key;

    if !keep_aspect {
        if handle.contains_east() {
            visual_w = (visual_w + local_dx).max(1.0);
        }
        if handle.contains_west() {
            let dw = (visual_w - 1.0).min(-local_dx);
            visual_x -= dw;
            visual_w += dw;
        }
        if handle.contains_south() {
            visual_h = (visual_h + local_dy).max(1.0);
        }
        if handle.contains_north() {
            let dh = (visual_h - 1.0).min(-local_dy);
            visual_y -= dh;
            visual_h += dh;
        }
    }

    if keep_aspect {
        if corner {
            let corners = layer_corners(transform, width, height);
            let index = match handle {
                Handle::Nw => 0,
                Handle::Ne => 1,
                Handle::Se => 2,
                Handle::Sw => 3,
                _ => 0,
            };
            let anchor_index = match handle {
                Handle::Nw => 2,
                Handle::Ne => 3,
                Handle::Se => 0,
                Handle::Sw => 1,
                _ => 0,
            };
            let visual_corner = corners[index];
            let visual_anchor = corners[anchor_index];
            let diag_x = visual_corner.x - visual_anchor.x;
            let diag_y = visual_corner.y - visual_anchor.y;
            let diag_len = (diag_x * diag_x + diag_y * diag_y).sqrt();
            if diag_len > 0.0 {
                let mut projected = (screen_dx * diag_x + screen_dy * diag_y) / diag_len;
                if alt_key {
                    projected *= 2.0;
                }
                let factor = (diag_len + projected) / diag_len;
                let factor = factor.max(1.0 / old_w).max(1.0 / old_h);
                visual_w = old_w * factor;
                visual_h = old_h * factor;
                if handle.contains_west() {
                    visual_x = transform.x + old_w - visual_w;
                }
                if handle.contains_north() {
                    visual_y = transform.y + old_h - visual_h;
                }
            }
        } else {
            let horizontal = handle == Handle::E || handle == Handle::W;
            let along = if horizontal {
                if handle == Handle::E {
                    local_dx
                } else {
                    -local_dx
                }
            } else if handle == Handle::S {
                local_dy
            } else {
                -local_dy
            };
            let base = if horizontal { old_w } else { old_h };
            let factor = (base + along) / base;
            let factor = factor.max(1.0 / old_w).max(1.0 / old_h);
            visual_w = old_w * factor;
            visual_h = old_h * factor;
            if handle == Handle::W {
                visual_x = transform.x + old_w - visual_w;
            }
            if handle == Handle::N {
                visual_y = transform.y + old_h - visual_h;
            }
            if horizontal {
                let old_center_y = transform.y + old_h / 2.0;
                visual_y = old_center_y - visual_h / 2.0;
            } else {
                let old_center_x = transform.x + old_w / 2.0;
                visual_x = old_center_x - visual_w / 2.0;
            }
        }
    }

    if alt_key {
        let old_center_x = transform.x + old_w / 2.0;
        let old_center_y = transform.y + old_h / 2.0;
        visual_x = old_center_x - visual_w / 2.0;
        visual_y = old_center_y - visual_h / 2.0;
    }

    if transform.rotation != 0.0 {
        let old_center_x = transform.x + old_w / 2.0;
        let old_center_y = transform.y + old_h / 2.0;
        let new_center_local_x = visual_x + visual_w / 2.0;
        let new_center_local_y = visual_y + visual_h / 2.0;
        let delta_local_x = new_center_local_x - old_center_x;
        let delta_local_y = new_center_local_y - old_center_y;
        let forward = transform.rotation * DEG;
        let (forward_cos, forward_sin) = (forward.cos(), forward.sin());
        let delta_visual_x = delta_local_x * forward_cos - delta_local_y * forward_sin;
        let delta_visual_y = delta_local_x * forward_sin + delta_local_y * forward_cos;
        let new_center_visual_x = old_center_x + delta_visual_x;
        let new_center_visual_y = old_center_y + delta_visual_y;
        visual_x = new_center_visual_x - visual_w / 2.0;
        visual_y = new_center_visual_y - visual_h / 2.0;
    }

    let sign_x = if transform.scale_x < 0.0 { -1.0 } else { 1.0 };
    let sign_y = if transform.scale_y < 0.0 { -1.0 } else { 1.0 };
    Transform2D {
        x: visual_x,
        y: visual_y,
        scale_x: sign_x * (visual_w / width),
        scale_y: sign_y * (visual_h / height),
        ..transform.clone()
    }
}

pub(crate) fn normalize_rotation(angle_deg: f64) -> f64 {
    let mut angle = angle_deg % 360.0;
    if angle > 180.0 {
        angle -= 360.0;
    }
    if angle < -180.0 {
        angle += 360.0;
    }
    angle
}

pub(crate) fn apply_rotation_drag(
    center: Point,
    start_point: Point,
    current_point: Point,
    start_rotation: f64,
    shift_key: bool,
) -> f64 {
    let start_angle = (start_point.y - center.y).atan2(start_point.x - center.x);
    let current_angle = (current_point.y - center.y).atan2(current_point.x - center.x);
    let delta = (current_angle - start_angle) * 180.0 / std::f64::consts::PI;
    let mut result = start_rotation + delta;
    if shift_key {
        result = (result / 15.0).round() * 15.0;
    }
    normalize_rotation(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(overrides: impl FnOnce(&mut Transform2D)) -> Transform2D {
        let mut transform = Transform2D {
            x: 100.0,
            y: 100.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
        };
        overrides(&mut transform);
        transform
    }

    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn center_and_corners_match_the_frozen_cases() {
        let transform = base(|_| {});
        let center = layer_center(&transform, 200.0, 100.0);
        close(center.x, 200.0);
        close(center.y, 150.0);
        let corners = layer_corners(&transform, 200.0, 100.0);
        assert_eq!(
            corners,
            [
                Point { x: 100.0, y: 100.0 },
                Point { x: 300.0, y: 100.0 },
                Point { x: 300.0, y: 200.0 },
                Point { x: 100.0, y: 200.0 },
            ]
        );

        let rotated = base(|t| t.rotation = 90.0);
        let corners = layer_corners(&rotated, 200.0, 100.0);
        close(corners[0].x, 250.0);
        close(corners[0].y, 50.0);
        close(corners[1].x, 250.0);
        close(corners[1].y, 250.0);
        close(corners[2].x, 150.0);
        close(corners[2].y, 250.0);
        close(corners[3].x, 150.0);
        close(corners[3].y, 50.0);

        let rotated = base(|t| t.rotation = 45.0);
        let corners = layer_corners(&rotated, 200.0, 100.0);
        close(corners[0].x, 164.64466164466164);
        close(corners[0].y, 43.93398193398193);
    }

    #[test]
    fn flip_does_not_change_the_visual_rect() {
        let plain = base(|_| {});
        let flipped = base(|t| t.flip_h = true);
        assert_eq!(
            layer_corners(&plain, 200.0, 100.0),
            layer_corners(&flipped, 200.0, 100.0)
        );
        assert_eq!(
            layer_aabb(&plain, 200.0, 100.0),
            layer_aabb(&flipped, 200.0, 100.0)
        );
    }

    #[test]
    fn document_local_round_trips_flip_and_scale() {
        let transform = base(|t| {
            t.scale_x = 2.0;
            t.flip_h = true;
            t.rotation = 30.0;
        });
        let local = document_to_layer_local(150.0, 140.0, &transform, 200.0, 100.0);
        assert!(local.x.is_finite() && local.y.is_finite());
        let center = layer_center(&transform, 200.0, 100.0);
        let at_center = document_to_layer_local(center.x, center.y, &transform, 200.0, 100.0);
        close(at_center.x, 100.0);
        close(at_center.y, 50.0);
    }

    #[test]
    fn detect_handle_selects_corners_sides_move_and_rotate() {
        let transform = base(|_| {});
        assert_eq!(
            detect_handle(Point { x: 100.0, y: 100.0 }, &transform, 200.0, 100.0, 1.0),
            Some(Handle::Nw)
        );
        assert_eq!(
            detect_handle(Point { x: 300.0, y: 150.0 }, &transform, 200.0, 100.0, 1.0),
            Some(Handle::E)
        );
        assert_eq!(
            detect_handle(Point { x: 200.0, y: 150.0 }, &transform, 200.0, 100.0, 1.0),
            Some(Handle::Move)
        );
        assert_eq!(
            detect_handle(Point { x: 200.0, y: 400.0 }, &transform, 200.0, 100.0, 1.0),
            Some(Handle::Rotate)
        );
        assert_eq!(
            detect_handle(
                Point {
                    x: -500.0,
                    y: -500.0
                },
                &transform,
                200.0,
                100.0,
                1.0
            ),
            None
        );
    }

    #[test]
    fn resize_keeps_flip_and_updates_scale() {
        let transform = base(|t| t.flip_h = true);
        let resized =
            apply_resize_handle(&transform, 200.0, 100.0, Handle::Se, 50.0, 0.0, true, false);
        assert!(resized.scale_x > 0.0);
        assert!(resized.flip_h);
        let resized = apply_resize_handle(
            &transform,
            200.0,
            100.0,
            Handle::Se,
            50.0,
            0.0,
            false,
            false,
        );
        assert!(resized.scale_x > 0.0);
        assert!(resized.flip_h);
    }

    #[test]
    fn rotation_drag_normalizes_and_snaps() {
        let center = Point { x: 0.0, y: 0.0 };
        let start = Point { x: 10.0, y: 0.0 };
        let current = Point { x: 0.0, y: 10.0 };
        close(
            apply_rotation_drag(center, start, current, 0.0, false),
            90.0,
        );
        close(apply_rotation_drag(center, start, current, 0.0, true), 90.0);
        close(
            apply_rotation_drag(center, start, Point { x: 10.0, y: 1.0 }, 179.0, false),
            normalize_rotation(179.0 + (1.0f64).atan2(10.0) * 180.0 / std::f64::consts::PI),
        );
    }
}

/// Corners of an axis-aligned rectangle rotated clockwise about its centre.
/// Order is top-left, top-right, bottom-right, bottom-left before rotation.
pub(crate) fn rotated_rect_corners(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    angle_deg: f64,
) -> [Point; 4] {
    let center = Point {
        x: x + width / 2.0,
        y: y + height / 2.0,
    };
    [
        Point { x, y },
        Point { x: x + width, y },
        Point {
            x: x + width,
            y: y + height,
        },
        Point { x, y: y + height },
    ]
    .map(|corner| rotate_point(corner, center, angle_deg))
}

/// The selection rotation pin: the top-edge midpoint plus the outward unit
/// normal of that edge, both in document space. The display pushes the pin
/// 24 screen pixels along the normal (`SelectionRenderer.tsx`).
pub(crate) fn selection_pin(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    angle_deg: f64,
) -> (Point, Point) {
    let corners = rotated_rect_corners(x, y, width, height, angle_deg);
    let top_mid = Point {
        x: (corners[0].x + corners[1].x) / 2.0,
        y: (corners[0].y + corners[1].y) / 2.0,
    };
    let center = Point {
        x: x + width / 2.0,
        y: y + height / 2.0,
    };
    let mut normal = Point {
        x: corners[1].y - corners[0].y,
        y: -(corners[1].x - corners[0].x),
    };
    let length = (normal.x * normal.x + normal.y * normal.y).sqrt();
    if length > f64::EPSILON {
        normal = Point {
            x: normal.x / length,
            y: normal.y / length,
        };
    }
    if normal.x * (top_mid.x - center.x) + normal.y * (top_mid.y - center.y) < 0.0 {
        normal = Point {
            x: -normal.x,
            y: -normal.y,
        };
    }
    (top_mid, normal)
}

/// Angle of a document point about a centre, in degrees (screen convention:
/// positive is clockwise because Y grows downward).
pub(crate) fn angle_about(center: Point, point: Point) -> f64 {
    (point.y - center.y).atan2(point.x - center.x) / DEG
}

/// The `cursorRotate.ts` rotate-cursor sprite: the oracle's 32-unit SVG path
/// (`M 1 7 L 10 1 L 10 5 A 16 16 ... Z`) flattened to a polygon and rotated
/// by `angle_deg` about the sprite centre, with the pointer angle offset the
/// source applies (`+45` because the base glyph tilts 45 degrees). Arcs are
/// sampled finely enough that a 24 px sprite shows no faceting.
pub(crate) fn rotate_cursor_polygon(angle_deg: f64) -> Vec<(f32, f32)> {
    fn arc_points(
        from: (f64, f64),
        to: (f64, f64),
        radius: f64,
        sweep: bool,
        out: &mut Vec<(f32, f32)>,
    ) {
        // SVG arc-to-centre (endpoint parameterization, phi = 0, y-down
        // screen space, large_arc = 0): nu = +1 when sweep differs from
        // large_arc, else -1.
        let (x1, y1) = from;
        let (x2, y2) = to;
        let dx2 = (x1 - x2) / 2.0;
        let dy2 = (y1 - y2) / 2.0;
        let d2 = dx2 * dx2 + dy2 * dy2;
        let num = radius * radius - d2;
        let nu = if sweep { 1.0 } else { -1.0 };
        let coef = if num > 0.0 && d2 > 0.0 {
            nu * (num / d2).sqrt()
        } else {
            0.0
        };
        let cx = coef * dy2 + (x1 + x2) / 2.0;
        let cy = -coef * dx2 + (y1 + y2) / 2.0;
        let a0 = (y1 - cy).atan2(x1 - cx);
        let a1 = (y2 - cy).atan2(x2 - cx);
        let mut delta = a1 - a0;
        if sweep && delta < 0.0 {
            delta += std::f64::consts::TAU;
        }
        if !sweep && delta > 0.0 {
            delta -= std::f64::consts::TAU;
        }
        let steps = ((delta.abs() / DEG).ceil() as usize).clamp(2, 32);
        for step in 1..steps {
            let t = a0 + delta * step as f64 / steps as f64;
            out.push((
                (cx + radius * t.cos()) as f32,
                (cy + radius * t.sin()) as f32,
            ));
        }
    }

    let mut points: Vec<(f32, f32)> = Vec::new();
    let mut current = (1.0_f64, 7.0_f64);
    points.push((1.0, 7.0));
    for (x, y) in [(10.0, 1.0), (10.0, 5.0)] {
        points.push((x as f32, y as f32));
        current = (x, y);
    }
    arc_points(current, (26.0, 21.0), 16.0, true, &mut points);
    current = (26.0, 21.0);
    for (x, y) in [(30.0, 21.0), (24.0, 30.0), (18.0, 21.0), (22.0, 21.0)] {
        points.push((x as f32, y as f32));
        current = (x, y);
    }
    arc_points(current, (10.0, 9.0), 12.0, false, &mut points);
    for (x, y) in [(10.0, 13.0)] {
        points.push((x as f32, y as f32));
    }
    let rad = (angle_deg + 45.0) * DEG;
    let (sin, cos) = rad.sin_cos();
    points
        .into_iter()
        .map(|(x, y)| {
            let px = f64::from(x) - 16.0;
            let py = f64::from(y) - 16.0;
            (
                (16.0 + px * cos - py * sin) as f32,
                (16.0 + px * sin + py * cos) as f32,
            )
        })
        .collect()
}
