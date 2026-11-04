use super::*;

use super::metrics::percentile;
use crate::app::measurement::InputMeasurementRender;

fn render(sample_id: u64, at: Instant) -> InputMeasurementRender {
    InputMeasurementRender {
        sample_id,
        doc_id: "doc".into(),
        version: 1,
        rendered_at: at,
    }
}

#[test]
fn percentile_reports_expected_quantiles() {
    let values = [1.0, 2.0, 3.0, 4.0];
    assert_eq!(percentile(&values, 0.5), 3.0);
    assert_eq!(percentile(&values, 0.95), 4.0);
}

#[test]
fn pixel_diff_counts_pixels_and_tiles() {
    let before = CanonicalState {
        doc_id: "doc".into(),
        layer_id: "layer".into(),
        width: 2,
        height: 1,
        epoch: 0,
        version: 0,
        pixels: vec![0, 0, 0, 0, 10, 10, 10, 10],
        foreground: [0, 0, 0, 255],
    };
    let after = CanonicalState {
        pixels: vec![1, 0, 0, 0, 10, 10, 10, 10],
        ..before.clone()
    };
    let (pixels, tiles) = pixel_diff(&before, &after).expect("diff succeeds");
    assert_eq!(pixels, 1);
    assert_eq!(tiles, 1);
}

#[test]
fn stroke_geometry_stays_inside_the_canvas() {
    for brush_size in [8, 40, 800] {
        let ((x0, y0), (x1, y1)) = stroke_geometry(11, 4096, 4096, brush_size);
        assert!(x0 <= x1 && x1 < 4096);
        assert!(y0 < 4096 && y1 < 4096);
    }
}

#[test]
fn wrong_window_draw_and_present_are_rejected() {
    let start = Instant::now();
    let rendered_at = start + Duration::from_millis(2);
    let events = [
        FrameRecord::Draw {
            window_id: 2_u8,
            start,
            end: start + Duration::from_millis(3),
        },
        FrameRecord::Present {
            window_id: 2_u8,
            end: start + Duration::from_millis(4),
        },
    ];
    assert!(matches!(
        correlate(&events, 1_u8, rendered_at),
        Correlation::Rejected(_)
    ));
}

#[test]
fn ambiguous_draw_association_is_rejected() {
    let start = Instant::now();
    let rendered_at = start + Duration::from_millis(1);
    let events = [
        FrameRecord::Draw {
            window_id: 1_u8,
            start,
            end: start + Duration::from_millis(3),
        },
        FrameRecord::Present {
            window_id: 1_u8,
            end: start + Duration::from_millis(4),
        },
        FrameRecord::Draw {
            window_id: 1_u8,
            start: start + Duration::from_millis(1),
            end: start + Duration::from_millis(5),
        },
    ];
    assert!(matches!(
        correlate(&events, 1_u8, rendered_at),
        Correlation::Rejected(_)
    ));
}

#[test]
fn matching_draw_and_following_present_correlate() {
    let start = Instant::now();
    let rendered_at = start + Duration::from_millis(1);
    let events = [
        FrameRecord::Draw {
            window_id: 1_u8,
            start,
            end: start + Duration::from_millis(2),
        },
        FrameRecord::Present {
            window_id: 1_u8,
            end: start + Duration::from_millis(3),
        },
    ];
    assert!(matches!(
        correlate(&events, 1_u8, rendered_at),
        Correlation::Matched(_)
    ));
    let early_present = [
        events[0],
        FrameRecord::Present {
            window_id: 1_u8,
            end: start + Duration::from_micros(1500),
        },
    ];
    assert!(matches!(
        correlate(&early_present, 1_u8, rendered_at),
        Correlation::Rejected(_)
    ));
}

#[test]
fn stale_render_timestamp_is_rejected() {
    let start = Instant::now();
    let events = [
        FrameRecord::Draw {
            window_id: 1_u8,
            start,
            end: start + Duration::from_millis(1),
        },
        FrameRecord::Present {
            window_id: 1_u8,
            end: start + Duration::from_millis(2),
        },
    ];
    assert!(matches!(
        correlate(&events, 1_u8, start + Duration::from_millis(3)),
        Correlation::Rejected(_)
    ));
}

#[test]
fn missing_present_before_next_draw_is_rejected() {
    let start = Instant::now();
    let events = [
        FrameRecord::Draw {
            window_id: 1_u8,
            start,
            end: start + Duration::from_millis(1),
        },
        FrameRecord::Draw {
            window_id: 1_u8,
            start: start + Duration::from_millis(2),
            end: start + Duration::from_millis(3),
        },
        FrameRecord::Present {
            window_id: 1_u8,
            end: start + Duration::from_millis(4),
        },
    ];
    assert!(matches!(
        correlate(&events, 1_u8, start),
        Correlation::Rejected(_)
    ));
}

#[test]
fn missing_receipt_is_rejected() {
    let at = Instant::now();
    let snapshot = InputMeasurementSnapshot {
        receipt: None,
        render: Some(render(1, at)),
    };
    assert!(validate_snapshot(1, &snapshot).is_err());
}
