//! # window_state - the window geometry carried between launches
//!
//! Owns: the on-disk shape of the geometry record, its validation, and the
//! pure placement rule that keeps a restored window reachable on the current
//! display layout.
//!
//! Must not own: reading or writing the settings file (`services::settings`),
//! opening or observing the window (`main`, `app`).

use gpui_kit::{point, px, size, Bounds, Pixels};

/// The settings key the record is stored under.
pub const KEY: &str = "photrez.window";

/// A window smaller than this is not a window the shell was ever laid out for;
/// the record is treated as corrupt rather than restored.
pub const MIN_W: f32 = 640.0;
pub const MIN_H: f32 = 480.0;

/// The largest edge accepted from the record. Anything above it is corrupt
/// data, not a window: restoring it would open a surface no display can show.
pub const MAX_EDGE: f32 = 8192.0;

/// The window's centre must stay on the chosen display. That is the placement
/// the platform validates - it rejects a window whose centre is on no monitor
/// and substitutes a full-display default - and it keeps at least half the
/// window reachable. One logical pixel of inset keeps the centre strictly
/// inside, because the monitor test is exclusive at the right/bottom edges.
pub const CENTRE_INSET: f32 = 1.0;

/// The persisted geometry, in the toolkit's logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeometry {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Whether the window was maximized. `x..h` then describe the restore
    /// bounds, which is the pair the platform itself remembers.
    pub maximized: bool,
}

impl WindowGeometry {
    /// The record for a live window's bounds.
    pub fn of(bounds: Bounds<Pixels>, maximized: bool) -> Self {
        Self {
            x: bounds.origin.x.as_f32(),
            y: bounds.origin.y.as_f32(),
            w: bounds.size.width.as_f32(),
            h: bounds.size.height.as_f32(),
            maximized,
        }
    }

    /// The window bounds to open at.
    pub fn bounds(self) -> Bounds<Pixels> {
        Bounds {
            origin: point(px(self.x), px(self.y)),
            size: size(px(self.w), px(self.h)),
        }
    }
}

/// The record's on-disk form: a flat JSON object with four numbers and a flag.
pub fn encode(geometry: WindowGeometry) -> String {
    serde_json::json!({
        "x": geometry.x,
        "y": geometry.y,
        "w": geometry.w,
        "h": geometry.h,
        "maximized": geometry.maximized,
    })
    .to_string()
}

/// Reads a record, or `None` when the text is not one. Every consumer treats
/// `None` as "open centered": a bad record must never keep the app from
/// starting.
///
/// Rejected: missing fields, non-numeric values, non-finite numbers (a value
/// beyond `f32`'s range converts to infinity), and sizes outside
/// [`MIN_W`]`x`[`MIN_H`] ..= [`MAX_EDGE`]. A record without the `maximized`
/// flag is read as a normal window, which is what such a record described.
pub fn decode(text: &str) -> Option<WindowGeometry> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let field = |name: &str| value.get(name).and_then(serde_json::Value::as_f64);
    let (x, y, w, h) = (
        field("x")? as f32,
        field("y")? as f32,
        field("w")? as f32,
        field("h")? as f32,
    );
    let finite = [x, y, w, h].iter().all(|v| v.is_finite());
    if !finite || w < MIN_W || h < MIN_H || w > MAX_EDGE || h > MAX_EDGE {
        return None;
    }
    let maximized = value
        .get("maximized")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    Some(WindowGeometry {
        x,
        y,
        w,
        h,
        maximized,
    })
}

/// Places a decoded record on the current display layout.
///
/// The display the record overlaps most wins, so a window parked on a second
/// monitor stays there - the returned index names that display, and the caller
/// passes it to the window so the toolkit validates the placement against the
/// right monitor. On that display the size is clamped to the display and the
/// origin is pulled inside until the window's centre is on the display: a
/// detached monitor or a resolution change leaves coordinates no monitor
/// covers, and a placement the platform rejects it replaces with a
/// full-display window.
/// Returns `None` when the record touches no display at all: the caller then
/// opens centered. The maximized flag passes through untouched; it describes a
/// window state, not a place.
pub fn restore(
    geometry: WindowGeometry,
    displays: &[Bounds<Pixels>],
) -> Option<(usize, WindowGeometry)> {
    let (index, (_, display)) = displays
        .iter()
        .map(|display| (overlap(geometry, *display), *display))
        .enumerate()
        .filter(|(_, (area, _))| *area > 0.0)
        .max_by(|(_, (a, _)), (_, (b, _))| a.total_cmp(b))?;

    let dx = display.origin.x.as_f32();
    let dy = display.origin.y.as_f32();
    let dw = display.size.width.as_f32();
    let dh = display.size.height.as_f32();

    let w = geometry.w.min(dw).max(MIN_W.min(dw));
    let h = geometry.h.min(dh).max(MIN_H.min(dh));
    // The centre is clamped inside the display; `w <= dw` (and `h <= dh`)
    // guarantees a non-empty range below.
    let centre_x = geometry.x + w / 2.0;
    let centre_y = geometry.y + h / 2.0;
    let centre_x = centre_x.clamp(dx + CENTRE_INSET, dx + dw - CENTRE_INSET);
    let centre_y = centre_y.clamp(dy + CENTRE_INSET, dy + dh - CENTRE_INSET);
    Some((
        index,
        WindowGeometry {
            x: centre_x - w / 2.0,
            y: centre_y - h / 2.0,
            w,
            h,
            maximized: geometry.maximized,
        },
    ))
}

/// The shared area of a record and a display, in logical px².
fn overlap(geometry: WindowGeometry, display: Bounds<Pixels>) -> f32 {
    let dx = display.origin.x.as_f32();
    let dy = display.origin.y.as_f32();
    let dw = display.size.width.as_f32();
    let dh = display.size.height.as_f32();
    let w = (geometry.x + geometry.w).min(dx + dw) - geometry.x.max(dx);
    let h = (geometry.y + geometry.h).min(dy + dh) - geometry.y.max(dy);
    if w <= 0.0 || h <= 0.0 {
        0.0
    } else {
        w * h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds {
            origin: point(px(x), px(y)),
            size: size(px(w), px(h)),
        }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> WindowGeometry {
        WindowGeometry {
            x,
            y,
            w,
            h,
            maximized: false,
        }
    }

    #[test]
    fn encode_then_decode_round_trips() {
        let original = WindowGeometry {
            maximized: true,
            ..rect(120.5, 64.0, 1366.0, 820.0)
        };
        assert_eq!(decode(&encode(original)), Some(original));
    }

    #[test]
    fn a_record_without_the_maximized_flag_is_a_normal_window() {
        let text = r#"{"x":100,"y":50,"w":1366,"h":820}"#;
        assert_eq!(
            decode(text),
            Some(rect(100.0, 50.0, 1366.0, 820.0)),
            "an older record must still restore"
        );
    }

    #[test]
    fn junk_and_wrong_shapes_decode_to_none() {
        for text in [
            "",
            "not json",
            "[]",
            "[1,2,3,4]",
            "null",
            r#"{"x":1,"y":2,"w":900}"#,           // missing field
            r#"{"x":"1","y":2,"w":900,"h":700}"#, // wrong value type
            r#"{"x":1,"y":2,"w":900,"h":null}"#,
        ] {
            assert_eq!(decode(text), None, "accepted {text:?}");
        }
    }

    #[test]
    fn non_finite_and_out_of_range_records_are_refused() {
        // 1e40 is a valid JSON number but overflows f32 to infinity.
        assert_eq!(
            decode(r#"{"x":1e40,"y":0,"w":900,"h":700}"#),
            None,
            "an overflowing coordinate must not become an infinite one"
        );
        assert_eq!(decode(r#"{"x":0,"y":0,"w":10,"h":700}"#), None);
        assert_eq!(decode(r#"{"x":0,"y":0,"w":900,"h":10}"#), None);
        assert_eq!(decode(r#"{"x":0,"y":0,"w":9000,"h":700}"#), None);
        assert_eq!(decode(r#"{"x":0,"y":0,"w":900,"h":9000}"#), None);
    }

    #[test]
    fn a_record_inside_a_display_keeps_its_place() {
        let displays = [display(0.0, 0.0, 1920.0, 1080.0)];
        let record = rect(100.0, 50.0, 1366.0, 820.0);
        assert_eq!(restore(record, &displays), Some((0, record)));
    }

    #[test]
    fn a_mostly_off_screen_record_is_pulled_back_until_its_centre_is_on_the_display() {
        let displays = [display(0.0, 0.0, 1920.0, 1080.0)];
        // Overlaps the display only at its left edge, and only by 10 px.
        let (_, placed) = restore(rect(1910.0, 500.0, 1100.0, 700.0), &displays)
            .expect("an overlapping record is restored");
        // The right overhang is allowed; what must hold is that the centre is
        // inside the display, which is the placement the platform accepts.
        assert_eq!(placed.x + placed.w / 2.0, 1920.0 - CENTRE_INSET);
        assert_eq!(placed.y, 500.0, "the axis that fits is not moved");
        assert_eq!((placed.w, placed.h), (1100.0, 700.0), "size is untouched");
    }

    #[test]
    fn a_negative_origin_on_the_display_to_the_left_is_kept() {
        let displays = [
            display(0.0, 0.0, 1920.0, 1080.0),
            display(-1280.0, 0.0, 1280.0, 1024.0),
        ];
        let record = rect(-1000.0, 40.0, 1100.0, 700.0);
        // The index names the monitor the record lives on, so the window can be
        // created against that monitor instead of the primary one.
        assert_eq!(restore(record, &displays), Some((1, record)));
    }

    #[test]
    fn an_oversized_record_is_clamped_to_the_display() {
        let displays = [display(0.0, 0.0, 1600.0, 900.0)];
        let (_, placed) = restore(rect(50.0, 40.0, 4000.0, 3000.0), &displays).unwrap();
        assert_eq!((placed.w, placed.h), (1600.0, 900.0));
        assert_eq!((placed.x, placed.y), (50.0, 40.0));
    }

    #[test]
    fn every_restored_placement_puts_the_window_centre_on_its_display() {
        let displays = [
            display(0.0, 0.0, 1920.0, 1080.0),
            display(-1280.0, 0.0, 1280.0, 1024.0),
        ];
        let records = [
            rect(-5000.0, -5000.0, 1100.0, 700.0),
            rect(5000.0, 5000.0, 1100.0, 700.0),
            rect(1900.0, 1000.0, 1100.0, 700.0),
            rect(-1270.0, 1000.0, 900.0, 700.0),
            rect(0.0, 0.0, 4000.0, 3000.0),
        ];
        for record in records {
            let Some((index, placed)) = restore(record, &displays) else {
                continue; // no overlap: the caller opens centered instead
            };
            let display = displays[index];
            let (cx, cy) = (placed.x + placed.w / 2.0, placed.y + placed.h / 2.0);
            let (left, top) = (display.origin.x.as_f32(), display.origin.y.as_f32());
            let (right, bottom) = (
                left + display.size.width.as_f32(),
                top + display.size.height.as_f32(),
            );
            assert!(
                cx > left && cx < right && cy > top && cy < bottom,
                "record {record:?} restored its centre outside display {index}"
            );
        }
    }

    #[test]
    fn a_record_that_touches_no_display_is_not_restored() {
        let displays = [display(0.0, 0.0, 1920.0, 1080.0)];
        // Far away and entirely above the display: a detached monitor leaves
        // coordinates no current display covers, and the caller's fallback is
        // a centered window.
        assert_eq!(
            restore(rect(5000.0, 4000.0, 1100.0, 700.0), &displays),
            None
        );
        assert_eq!(restore(rect(100.0, -2000.0, 900.0, 700.0), &displays), None);
        assert_eq!(restore(rect(100.0, 50.0, 900.0, 700.0), &[]), None);
    }

    #[test]
    fn the_restored_window_keeps_the_maximized_state() {
        let displays = [display(0.0, 0.0, 1920.0, 1080.0)];
        let record = WindowGeometry {
            maximized: true,
            ..rect(100.0, 50.0, 1366.0, 820.0)
        };
        let (_, placed) = restore(record, &displays).unwrap();
        assert!(placed.maximized, "the state is not lost on the way back");
        assert_eq!((placed.x, placed.y), (100.0, 50.0));
    }

    #[test]
    fn bounds_and_the_record_convert_both_ways() {
        let original = rect(12.0, 34.0, 640.0, 480.0);
        assert_eq!(WindowGeometry::of(original.bounds(), false), original);
    }
}
