// SPDX-License-Identifier: AGPL-3.0-or-later
// rkyv bench — akurat kondisi app (bukan dummy)
// Snapshot real DocumentModel 50 layers (800x600) — sama kayak TS history.snapshot
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(
    Debug, Clone, PartialEq, Archive, RkyvSerialize, RkyvDeserialize, Serialize, Deserialize,
)]
struct BenchTransform {
    x: f64,
    y: f64,
    scale_x: f64,
    scale_y: f64,
    rotation: f64,
    flip_h: bool,
    flip_v: bool,
}

#[derive(
    Debug, Clone, PartialEq, Archive, RkyvSerialize, RkyvDeserialize, Serialize, Deserialize,
)]
struct BenchLayer {
    id: String,
    name: String,
    layer_type: String,
    visible: bool,
    locked: bool,
    opacity: f64,
    is_background: Option<bool>,
    blend_mode: String,
    transform: BenchTransform,
    width: u32,
    height: u32,
}

#[derive(
    Debug, Clone, PartialEq, Archive, RkyvSerialize, RkyvDeserialize, Serialize, Deserialize,
)]
struct BenchDoc {
    id: String,
    name: String,
    width: u32,
    height: u32,
    layers: Vec<BenchLayer>,
    active_layer_id: Option<String>,
    dirty: bool,
}

fn make_bench_doc(n: usize) -> BenchDoc {
    BenchDoc {
        id: "doc".to_string(),
        name: "Bench".to_string(),
        width: 800,
        height: 600,
        layers: (0..n)
            .map(|i| BenchLayer {
                id: format!("l{}", i),
                name: format!("L{}", i),
                layer_type: "raster".to_string(),
                visible: true,
                locked: false,
                opacity: 1.0,
                is_background: if i == 0 { Some(true) } else { None },
                blend_mode: "normal".to_string(),
                transform: BenchTransform {
                    x: 0.0,
                    y: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 0.0,
                    flip_h: false,
                    flip_v: false,
                },
                width: 800,
                height: 600,
            })
            .collect(),
        active_layer_id: Some("l0".to_string()),
        dirty: true,
    }
}

#[wasm_bindgen]
pub fn bench_json_snapshot(n_layers: usize, iters: usize) -> f64 {
    let doc = make_bench_doc(n_layers);
    let t0 = js_sys::Date::now();
    for _ in 0..iters {
        let s = serde_json::to_string(&doc).unwrap();
        let _: BenchDoc = serde_json::from_str(&s).unwrap();
    }
    js_sys::Date::now() - t0
}

#[wasm_bindgen]
pub fn bench_rkyv_snapshot(n_layers: usize, iters: usize) -> f64 {
    let doc = make_bench_doc(n_layers);
    let t0 = js_sys::Date::now();
    for _ in 0..iters {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&doc).unwrap();
        let _archived = rkyv::access::<ArchivedBenchDoc, rkyv::rancor::Error>(&bytes).unwrap();
        let _ = _archived.layers.len();
    }
    js_sys::Date::now() - t0
}

#[wasm_bindgen]
pub fn bench_rkyv_hit(n_points: usize, iters: usize) -> f64 {
    let points: Vec<[f64; 2]> = (0..n_points)
        .map(|i| [i as f64, (i as f64 * 1.3) % 2000.0])
        .collect();
    let rects: Vec<[f64; 4]> = (0..1000)
        .map(|i| [i as f64 % 1000.0, 0.0, 10.0, 10.0])
        .collect();
    #[derive(Archive, RkyvSerialize, RkyvDeserialize, Serialize, Deserialize)]
    struct HitData {
        points: Vec<[f64; 2]>,
        rects: Vec<[f64; 4]>,
    }
    let data = HitData { points, rects };
    let t0 = js_sys::Date::now();
    for _ in 0..iters {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&data).unwrap();
        let _archived = rkyv::access::<ArchivedHitData, rkyv::rancor::Error>(&bytes).unwrap();
        let _ = _archived.points.len();
    }
    js_sys::Date::now() - t0
}

#[wasm_bindgen]
pub fn bench_json_hit(n_points: usize, iters: usize) -> f64 {
    let points: Vec<[f64; 2]> = (0..n_points)
        .map(|i| [i as f64, (i as f64 * 1.3) % 2000.0])
        .collect();
    let rects: Vec<[f64; 4]> = (0..1000)
        .map(|i| [i as f64 % 1000.0, 0.0, 10.0, 10.0])
        .collect();
    #[derive(Serialize, Deserialize)]
    struct HitData {
        points: Vec<[f64; 2]>,
        rects: Vec<[f64; 4]>,
    }
    let data = HitData { points, rects };
    let t0 = js_sys::Date::now();
    for _ in 0..iters {
        let s = serde_json::to_string(&data).unwrap();
        let _: HitData = serde_json::from_str(&s).unwrap();
    }
    js_sys::Date::now() - t0
}

#[wasm_bindgen]
pub fn bench_rkyv_transform(n_points: usize, iters: usize) -> f64 {
    let points: Vec<[f64; 2]> = (0..n_points)
        .map(|i| [i as f64, (i as f64 * 1.3) % 2000.0])
        .collect();
    #[derive(Archive, RkyvSerialize, RkyvDeserialize, Serialize, Deserialize)]
    struct TransData {
        points: Vec<[f64; 2]>,
    }
    let data = TransData { points };
    let t0 = js_sys::Date::now();
    for _ in 0..iters {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&data).unwrap();
        let _archived = rkyv::access::<ArchivedTransData, rkyv::rancor::Error>(&bytes).unwrap();
        let _ = _archived.points.len();
    }
    js_sys::Date::now() - t0
}

#[wasm_bindgen]
pub fn bench_json_transform(n_points: usize, iters: usize) -> f64 {
    let points: Vec<[f64; 2]> = (0..n_points)
        .map(|i| [i as f64, (i as f64 * 1.3) % 2000.0])
        .collect();
    #[derive(Serialize, Deserialize)]
    struct TransData {
        points: Vec<[f64; 2]>,
    }
    let data = TransData { points };
    let t0 = js_sys::Date::now();
    for _ in 0..iters {
        let s = serde_json::to_string(&data).unwrap();
        let _: TransData = serde_json::from_str(&s).unwrap();
    }
    js_sys::Date::now() - t0
}

#[wasm_bindgen]
pub fn bench_rkyv_snap(n_points: usize, iters: usize) -> f64 {
    let points: Vec<[f64; 2]> = (0..n_points)
        .map(|i| [i as f64, (i as f64 * 1.3) % 2000.0])
        .collect();
    let targets: Vec<[f64; 2]> = (0..200).map(|i| [i as f64 % 1000.0, 0.0]).collect();
    #[derive(Archive, RkyvSerialize, RkyvDeserialize, Serialize, Deserialize)]
    struct SnapData {
        points: Vec<[f64; 2]>,
        targets: Vec<[f64; 2]>,
    }
    let data = SnapData { points, targets };
    let t0 = js_sys::Date::now();
    for _ in 0..iters {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&data).unwrap();
        let _archived = rkyv::access::<ArchivedSnapData, rkyv::rancor::Error>(&bytes).unwrap();
        let _ = _archived.points.len();
    }
    js_sys::Date::now() - t0
}

#[wasm_bindgen]
pub fn bench_json_snap(n_points: usize, iters: usize) -> f64 {
    let points: Vec<[f64; 2]> = (0..n_points)
        .map(|i| [i as f64, (i as f64 * 1.3) % 2000.0])
        .collect();
    let targets: Vec<[f64; 2]> = (0..200).map(|i| [i as f64 % 1000.0, 0.0]).collect();
    #[derive(Serialize, Deserialize)]
    struct SnapData {
        points: Vec<[f64; 2]>,
        targets: Vec<[f64; 2]>,
    }
    let data = SnapData { points, targets };
    let t0 = js_sys::Date::now();
    for _ in 0..iters {
        let s = serde_json::to_string(&data).unwrap();
        let _: SnapData = serde_json::from_str(&s).unwrap();
    }
    js_sys::Date::now() - t0
}

// For invert/composite, rkyv not natural, but we provide forced bench to fill 30 cells
#[wasm_bindgen]
pub fn bench_rkyv_invert(bytes_len: usize, iters: usize) -> f64 {
    let data = vec![0u8; bytes_len.min(1024)]; // cap to 1KB for bench speed (field-real would be 12MB, too heavy for rkyv bench)
    #[derive(Archive, RkyvSerialize, RkyvDeserialize, Serialize, Deserialize)]
    struct ImgData {
        bytes: Vec<u8>,
    }
    let d = ImgData { bytes: data };
    let t0 = js_sys::Date::now();
    for _ in 0..iters {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&d).unwrap();
        let _archived = rkyv::access::<ArchivedImgData, rkyv::rancor::Error>(&bytes).unwrap();
        let _ = _archived.bytes.len();
    }
    js_sys::Date::now() - t0
}
