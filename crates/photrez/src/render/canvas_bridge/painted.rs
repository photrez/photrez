use super::*;

pub(super) fn painted_circle(
    window: &mut gpui_kit::Window,
    center: (f32, f32),
    radius: f32,
    stroke: Option<f32>,
    color: gpui_kit::Hsla,
) {
    let mut path = match stroke {
        Some(width) => PathBuilder::stroke(px(width)),
        None => PathBuilder::fill(),
    };
    for step in 0..=48 {
        let t = step as f32 / 48.0 * std::f32::consts::TAU;
        let (sin, cos) = t.sin_cos();
        let p = point(px(center.0 + radius * cos), px(center.1 + radius * sin));
        if step == 0 {
            path.move_to(p);
        } else {
            path.line_to(p);
        }
    }
    path.close();
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

/// The rotate-cursor sprite: the 32-unit glyph at 0.75 scale (24 px),
/// its (1, 7) hotspot anchored at the pointer, white fill with a black
/// 1.2-unit stroke, rotated with the pointer angle.
pub(super) fn painted_rotate_cursor(
    window: &mut gpui_kit::Window,
    position: (f32, f32),
    angle: f32,
) {
    let scale: f32 = 0.75;
    let polygon = crate::app::transform_geometry::rotate_cursor_polygon(f64::from(angle));
    let mut fill = PathBuilder::fill();
    let mut stroke = PathBuilder::stroke(px(1.2 * scale));
    for (index, (x, y)) in polygon.iter().enumerate() {
        let p = point(
            px(position.0 - 1.0 * scale + *x * scale),
            px(position.1 - 7.0 * scale + *y * scale),
        );
        if index == 0 {
            fill.move_to(p);
            stroke.move_to(p);
        } else {
            fill.line_to(p);
            stroke.line_to(p);
        }
    }
    fill.close();
    stroke.close();
    if let Ok(path) = fill.build() {
        window.paint_path(path, hsla(0., 0., 1., 1.));
    }
    if let Ok(path) = stroke.build() {
        window.paint_path(path, hsla(0., 0., 0., 1.));
    }
}

/// A white square handle with an accent rim, centred on a window point.
pub(super) fn painted_handle(window: &mut gpui_kit::Window, center: (f32, f32), edge: f32) {
    let bounds = Bounds {
        origin: point(px(center.0 - edge / 2.), px(center.1 - edge / 2.)),
        size: size(px(edge), px(edge)),
    };
    window.paint_quad(fill(bounds, hsla(0., 0., 1., 1.)));
    window.paint_quad(outline(bounds, color::ACCENT, BorderStyle::Solid));
}

pub(super) fn painted_dashed_line(
    window: &mut gpui_kit::Window,
    from: (f32, f32),
    to: (f32, f32),
    width: f32,
    dash: f32,
    gap: f32,
    color: gpui_kit::Hsla,
) {
    painted_dashed_line_offset(window, from, to, width, dash, gap, 0.0, color);
}

/// `painted_dashed_line` with the pattern shifted by `offset` px along the
/// line, so a second stroke can interleave with the first (`stroke-dashoffset`).
#[allow(clippy::too_many_arguments)] // Coordinate-heavy rendering keeps endpoints and dash parameters explicit.
fn painted_dashed_line_offset(
    window: &mut gpui_kit::Window,
    from: (f32, f32),
    to: (f32, f32),
    width: f32,
    dash: f32,
    gap: f32,
    offset: f32,
    color: gpui_kit::Hsla,
) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = (dx * dx + dy * dy).sqrt();
    if length <= f32::EPSILON {
        return;
    }
    let (ux, uy) = (dx / length, dy / length);
    let mut path = PathBuilder::stroke(px(width));
    let period = dash + gap;
    let phase = offset.rem_euclid(period);
    let mut travelled = -(period - phase);
    while travelled < length {
        let start = travelled.max(0.0);
        let end = (travelled + dash).min(length);
        if end > start {
            path.move_to(point(px(from.0 + ux * start), px(from.1 + uy * start)));
            path.line_to(point(px(from.0 + ux * end), px(from.1 + uy * end)));
        }
        travelled += period;
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

/// `painted_dashed_line` walked along a polyline: the dash pattern continues
/// across corners, so a whole marquee needs one tessellated path per layer
/// instead of one per edge. A dash running past a corner loses its remainder
/// (at most one dash length per corner).
pub(super) fn painted_dashed_polyline(
    window: &mut gpui_kit::Window,
    points: &[(f32, f32)],
    width: f32,
    dash: f32,
    gap: f32,
    offset: f32,
    color: gpui_kit::Hsla,
) {
    if points.len() < 2 {
        return;
    }
    let period = dash + gap;
    let mut path = PathBuilder::stroke(px(width));
    // Arc distance until the next dash boundary, relative to each edge start.
    let mut position = -offset.rem_euclid(period);
    let mut first = true;
    for pair in points.windows(2) {
        let from = (pair[0].0, pair[0].1);
        let to = (pair[1].0, pair[1].1);
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = (dx * dx + dy * dy).sqrt();
        if length <= f32::EPSILON {
            continue;
        }
        let (ux, uy) = (dx / length, dy / length);
        while position < length {
            let start = position.max(0.0);
            let end = (position + dash).min(length);
            if end > start {
                let a = point(px(from.0 + ux * start), px(from.1 + uy * start));
                let b = point(px(from.0 + ux * end), px(from.1 + uy * end));
                if first {
                    path.move_to(a);
                    first = false;
                } else {
                    path.line_to(a);
                }
                path.line_to(b);
            }
            position += period;
        }
        position -= length;
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

pub(super) fn painted_dashed_circle(
    window: &mut gpui_kit::Window,
    center: (f32, f32),
    radius: f32,
    width: f32,
    dash: f32,
    gap: f32,
    color: gpui_kit::Hsla,
) {
    painted_dashed_ellipse(window, center, radius, radius, width, dash, gap, 0.0, color);
}

/// A dashed axis-aligned ellipse (`rx`/`ry` in window px) whose dash pattern
/// starts `offset` px along the perimeter, so a white stroke can interleave
/// with an accent one.
#[allow(clippy::too_many_arguments)] // one knob per SelectionRenderer stroke property
pub(super) fn painted_dashed_ellipse(
    window: &mut gpui_kit::Window,
    center: (f32, f32),
    rx: f32,
    ry: f32,
    width: f32,
    dash: f32,
    gap: f32,
    offset: f32,
    color: gpui_kit::Hsla,
) {
    if rx <= f32::EPSILON || ry <= f32::EPSILON {
        return;
    }
    // Ramanujan's approximation keeps the dash period in real px.
    let perimeter = std::f32::consts::PI
        * (3.0 * (rx + ry) - ((3.0 * rx + ry) * (rx + 3.0 * ry)).sqrt()).max(1.0);
    let segments = (perimeter / (dash + gap)).ceil().max(4.0) as usize;
    let step = std::f32::consts::TAU / segments as f32;
    let on = step * dash / (dash + gap);
    let phase = (offset / (dash + gap)).rem_euclid(1.0) * step;
    let mut path = PathBuilder::stroke(px(width));
    for segment in 0..segments {
        let start = segment as f32 * step + phase;
        let mut first = true;
        for sub in 0..=4 {
            let t = start + on * sub as f32 / 4.0;
            let (sin, cos) = t.sin_cos();
            let p = point(px(center.0 + rx * cos), px(center.1 + ry * sin));
            if first {
                path.move_to(p);
                first = false;
            } else {
                path.line_to(p);
            }
        }
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

/// Point `distance` document units from `from` towards `to`.
pub(super) fn step_towards(from: [f64; 2], to: [f64; 2], distance: f64) -> [f64; 2] {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let length = (dx * dx + dy * dy).sqrt();
    if length <= f64::EPSILON {
        return from;
    }
    let t = (distance / length).min(1.0);
    [from[0] + dx * t, from[1] + dy * t]
}

pub(super) fn doc_point_to_window(
    page_x: f32,
    page_y: f32,
    page_w: f32,
    page_h: f32,
    doc_w: f32,
    doc_h: f32,
    point_doc: [f64; 2],
) -> Option<Point<Pixels>> {
    if !page_w.is_finite() || !page_h.is_finite() || page_w <= 0. || page_h <= 0. {
        return None;
    }
    let x = page_x + page_w * (point_doc[0] as f32 / doc_w);
    let y = page_y + page_h * (point_doc[1] as f32 / doc_h);
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    Some(point(px(x), px(y)))
}

pub(super) fn doc_rect_to_window(
    page_x: f32,
    page_y: f32,
    page_w: f32,
    page_h: f32,
    doc_w: f32,
    doc_h: f32,
    rect: (f64, f64, f64, f64),
) -> Option<Bounds<Pixels>> {
    if !page_w.is_finite() || !page_h.is_finite() || page_w <= 0. || page_h <= 0. {
        return None;
    }
    let (x, y, width, height) = rect;
    if ![x, y, width, height].iter().all(|value| value.is_finite()) {
        return None;
    }
    let left = page_x + page_w * (x as f32 / doc_w);
    let top = page_y + page_h * (y as f32 / doc_h);
    let right = page_x + page_w * ((x + width) as f32 / doc_w);
    let bottom = page_y + page_h * ((y + height) as f32 / doc_h);
    if !left.is_finite() || !top.is_finite() || right <= left || bottom <= top {
        return None;
    }
    Some(Bounds {
        origin: point(px(left), px(top)),
        size: size(px(right - left), px(bottom - top)),
    })
}
