// SPDX-License-Identifier: AGPL-3.0-or-later
//! Serialization cost of the `.ptz` `document.json`, measured through the
//! PRODUCTION writer (`PtzDocument::to_json`).
//!
//! IGNORED BY DEFAULT — `cargo test` skips `#[ignore]`, so this never runs in
//! the normal gates. Run it deliberately:
//!
//! ```text
//! cargo test -p photrez-core --lib --release -- --ignored --nocapture ptz_save_perf
//! ```
//!
//! WHAT IT DOES AND DOES NOT MEASURE. It times the document JSON only: the
//! parse of the host payload and the serialize of the bytes written to disk.
//! It does NOT time PNG encoding, which is the dominant cost of a real save and
//! is unchanged by this work (pixel bytes never pass through this path). So the
//! numbers here are an upper bound on the overhead this change adds, not a
//! measurement of save duration; multiplying them by a layer count and calling
//! the result "the save" would be wrong.
//!
//! The documents below are built to the editor's `MAX_LAYERS` (200) ceiling,
//! which is the realistic worst case for layer count.
//!
//! Two layer shapes are measured, because they are not the same cost:
//!   - `full_metadata`: every nested payload populated (adjustment + shape +
//!     text). The true worst case for serialization work per layer.
//!   - `raster_only`: one adjustment and no shape/text, which is what a typical
//!     painting document looks like. Cheaper, and the common case.

#![cfg(test)]

use crate::ptz_document::PtzDocument;
use std::time::Instant;

const ITERS: u32 = 200;

fn transform() -> serde_json::Value {
    serde_json::json!({
        "x": 12.5, "y": -8.25, "scaleX": 1.5, "scaleY": 1.25,
        "rotation": 33.0, "flipH": true, "flipV": false
    })
}

fn raster_layer(i: usize, with_adjustment: bool) -> serde_json::Value {
    serde_json::json!({
        "id": format!("layer-{:04}", i),
        "name": format!("Layer {}", i),
        "type": "raster",
        "visible": true,
        "opacity": 0.75,
        "locked": false,
        "isBackground": i == 0,
        "lockTransparency": false,
        "lockPosition": false,
        "lockRotation": false,
        "hasAdjustments": with_adjustment,
        "basicAdjustment": if with_adjustment {
            serde_json::json!({"brightness": 5.0, "contrast": -10.0, "saturation": 20.0})
        } else {
            serde_json::Value::Null
        },
        "baseImageBitmap": null,
        "blendMode": "multiply",
        "transform": transform(),
        "width": 1920.0,
        "height": 1080.0,
        "imageBitmap": null
    })
}

fn shape_layer(i: usize) -> serde_json::Value {
    serde_json::json!({
        "id": format!("shape-{:04}", i),
        "name": format!("Shape {}", i),
        "type": "shape",
        "visible": true,
        "opacity": 1.0,
        "locked": false,
        "blendMode": "normal",
        "transform": transform(),
        "width": 400.0,
        "height": 300.0,
        "shapeParams": {
            "kind": "star", "width": 400.0, "height": 300.0, "radius": 11.5,
            "fill": {"kind": "solid", "color": "#E15A17"},
            "stroke": {"enabled": true, "color": "#00FF00", "width": 3.5},
            "arrowHead": false
        }
    })
}

fn text_layer(i: usize) -> serde_json::Value {
    serde_json::json!({
        "id": format!("text-{:04}", i),
        "name": format!("Text {}", i),
        "type": "text",
        "visible": true,
        "opacity": 1.0,
        "locked": false,
        "blendMode": "normal",
        "transform": transform(),
        "width": 300.0,
        "height": 40.0,
        "textData": {
            "content": "Sample headline text", "fontFamily": "Inter",
            "fontSize": 72.0, "fontWeight": 700.0, "fontStyle": "italic",
            "color": "#123456", "align": "center", "lineHeight": 2.5,
            "letterSpacing": 3.5, "boxMode": "area", "boxWidth": 320.0,
            "boxHeight": 44.0,
            "stroke": {"width": 2.5, "color": "#ABCDEF", "align": "inside"},
            "underline": true, "strikethrough": false, "uppercase": true
        }
    })
}

fn document_json(shape: &str, n: usize) -> String {
    let layers: Vec<serde_json::Value> = (0..n)
        .map(|i| match shape {
            "full_metadata" => {
                // Rotate through the three content kinds so every nested
                // payload is exercised at least once.
                match i % 3 {
                    0 => raster_layer(i, true),
                    1 => shape_layer(i),
                    _ => text_layer(i),
                }
            }
            _ => raster_layer(i, true),
        })
        .collect();
    serde_json::json!({
        "id": "perf-doc",
        "name": "Perf",
        "width": 1920.0,
        "height": 1080.0,
        "activeLayerId": "layer-0000",
        "selection": {"x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0, "angle": 0.0},
        "viewport": {"panX": 40.0, "panY": 63.6, "zoom": 2.684, "rotation": 0.0},
        "dirty": false,
        "layers": layers,
        "format": "photrez-ptz",
        "version": 3
    })
    .to_string()
}

#[test]
#[ignore = "perf measurement; run explicitly"]
fn ptz_save_perf_document_json_cost() {
    for shape in ["raster_only", "full_metadata"] {
        for layer_count in [1usize, 50, 200] {
            let host_value: serde_json::Value =
                serde_json::from_str(&document_json(shape, layer_count)).unwrap();

            // Deserialize: what the command does with the IPC payload.
            let t0 = Instant::now();
            for _ in 0..ITERS {
                let _: PtzDocument = serde_json::from_value(host_value.clone()).unwrap();
            }
            let parse_each = t0.elapsed() / ITERS;

            let doc: PtzDocument = serde_json::from_value(host_value).unwrap();

            // Serialize: the bytes that actually go to disk.
            let t0 = Instant::now();
            let mut out_len = 0usize;
            for _ in 0..ITERS {
                out_len = doc.to_json().unwrap().len();
            }
            let write_each = t0.elapsed() / ITERS;

            println!(
                "PTZ_PERF shape={:<13} layers={:>3} bytes={:>7} parse={:>10?} write={:>10?} total={:>10?}",
                shape,
                layer_count,
                out_len,
                parse_each,
                write_each,
                parse_each + write_each
            );
        }
    }
    eprintln!(
        "NOTE: document JSON only. PNG encoding dominates a real save and is \
         unaffected by the writer, so these are NOT save-duration numbers."
    );
}
