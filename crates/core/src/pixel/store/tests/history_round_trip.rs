// SPDX-License-Identifier: AGPL-3.0-or-later

use super::reg;
use crate::raster::paint::raster::{fnv1a, RasterDab, RasterTip};
use crate::raster::tile::TilePatch;

const W: usize = 4;
const H: usize = 4;
const BASE: [u8; 4] = [40, 80, 120, 255];
const BRUSH: [u8; 4] = [200, 30, 60, 255];

fn base_pixels() -> Vec<u8> {
    BASE.iter().copied().cycle().take(W * H * 4).collect()
}

fn pixel(pixels: &[u8], x: usize, y: usize) -> [u8; 4] {
    let offset = (y * W + x) * 4;
    pixels[offset..offset + 4]
        .try_into()
        .expect("pixel slice has four bytes")
}

fn splice(base: &[u8], tiles: &[TilePatch]) -> Vec<u8> {
    let mut out = base.to_vec();
    for tile in tiles {
        for row in 0..tile.h {
            let dst = ((tile.y + row as i64) as usize * W + tile.x as usize) * 4;
            let src = row * tile.w * 4;
            out[dst..dst + tile.w * 4].copy_from_slice(&tile.data[src..src + tile.w * 4]);
        }
    }
    out
}

#[test]
fn non_white_brush_and_eraser_round_trip_through_history() {
    let mut r = reg();
    r.open_document("g14");
    r.add_layer("g14", "L", W as u32, H as u32, base_pixels())
        .expect("non-white base layer seeds");

    let brush_tip_data: Vec<u8> = BRUSH.iter().copied().cycle().take(2 * 2 * 4).collect();
    let brush_tip = RasterTip {
        width: 2,
        height: 2,
        data: &brush_tip_data,
    };
    let brush_dabs = [RasterDab {
        x: 1.0,
        y: 1.0,
        alpha: 1.0,
    }];
    let base = base_pixels();
    let (brush_before, brush_after, _, brush_version) = r
        .commit_pixels(
            "g14",
            "L",
            base.clone(),
            W,
            H,
            false,
            2.0,
            &brush_dabs,
            &brush_tip,
            true,
        )
        .expect("brush commit");

    assert_eq!(splice(&base, &brush_before), base, "brush pre-image");
    let painted = r.get_layer("g14", "L").unwrap().pixels.clone();
    assert_eq!(
        splice(&base, &brush_after),
        painted,
        "brush after-tiles must recomposite to the canonical bytes"
    );
    assert_eq!(pixel(&painted, 0, 0), BRUSH);
    assert_eq!(pixel(&painted, 1, 1), BRUSH);
    assert_eq!(pixel(&painted, 2, 2), BASE);
    assert_eq!(r.get_history_cursor("g14"), Some(1));
    assert_eq!(brush_version, 1);
    assert_eq!(r.get_history_version("g14"), Some(1));

    let (_, _, _, undo_version) = r.undo_pixel("g14").expect("brush undo");
    assert_eq!(r.get_layer("g14", "L").unwrap().pixels, base);
    assert_eq!(r.get_history_cursor("g14"), Some(0));
    assert_eq!(undo_version, 2);
    assert_eq!(r.get_history_version("g14"), Some(2));

    let (_, _, _, redo_version) = r.redo_pixel("g14").expect("brush redo");
    assert_eq!(r.get_layer("g14", "L").unwrap().pixels, painted);
    assert_eq!(r.get_history_cursor("g14"), Some(1));
    assert_eq!(redo_version, 3);
    assert_eq!(r.get_history_version("g14"), Some(3));

    let eraser_tip_data = [0u8, 0, 0, 255];
    let eraser_tip = RasterTip {
        width: 1,
        height: 1,
        data: &eraser_tip_data,
    };
    let eraser_dabs = [RasterDab {
        x: 1.5,
        y: 1.5,
        alpha: 1.0,
    }];
    let (erase_before, erase_after, _, erase_version) = r
        .commit_pixels(
            "g14",
            "L",
            painted.clone(),
            W,
            H,
            true,
            1.0,
            &eraser_dabs,
            &eraser_tip,
            true,
        )
        .expect("eraser commit");

    assert_eq!(splice(&painted, &erase_before), painted, "eraser pre-image");
    let erased = r.get_layer("g14", "L").unwrap().pixels.clone();
    assert_eq!(
        splice(&painted, &erase_after),
        erased,
        "eraser after-tiles must recomposite to the canonical bytes"
    );
    assert_ne!(erased, painted);
    assert_eq!(pixel(&erased, 1, 1), [0, 0, 0, 0]);
    assert_eq!(pixel(&erased, 0, 0), BRUSH);
    assert_eq!(r.get_history_cursor("g14"), Some(2));
    assert_eq!(erase_version, 4);
    assert_eq!(r.get_history_version("g14"), Some(4));

    let (_, _, _, undo_erase_version) = r.undo_pixel("g14").expect("eraser undo");
    assert_eq!(r.get_layer("g14", "L").unwrap().pixels, painted);
    assert_eq!(r.get_history_cursor("g14"), Some(1));
    assert_eq!(undo_erase_version, 5);
    assert_eq!(r.get_history_version("g14"), Some(5));

    let (_, _, _, redo_erase_version) = r.redo_pixel("g14").expect("eraser redo");
    assert_eq!(r.get_layer("g14", "L").unwrap().pixels, erased);
    assert_eq!(r.get_history_cursor("g14"), Some(2));
    assert_eq!(redo_erase_version, 6);
    assert_eq!(r.get_history_version("g14"), Some(6));
}

/// Golden-byte undo/redo determinism: a layer patched via `write_region`
/// must rebuild the EXACT same pixel digest after undo -> redo.
#[test]
fn undo_redo_rebuilt_digest_byte_identical() {
    use crate::pixel::store::PixelStoreRegistry;

    fn layer_digest(pixels: &[u8], w: usize, h: usize) -> String {
        let mut tile_hashes: Vec<(String, String)> = Vec::new();
        for ty in 0..h.div_ceil(256) {
            for tx in 0..w.div_ceil(256) {
                let tw = 256.min(w - tx * 256);
                let th = 256.min(h - ty * 256);
                let mut data = Vec::with_capacity(tw * th * 4);
                for row in 0..th {
                    let off = ((ty * 256 + row) * w + tx * 256) * 4;
                    data.extend_from_slice(&pixels[off..off + tw * 4]);
                }
                tile_hashes.push((format!("{tx},{ty}"), format!("{:08x}", fnv1a(&data))));
            }
        }
        tile_hashes.sort_by(|a, b| a.0.cmp(&b.0));
        let combined: String = tile_hashes
            .iter()
            .map(|(k, h)| format!("{k}:{h};"))
            .collect();
        format!("{:08x}", fnv1a(combined.as_bytes()))
    }

    const N: usize = 64;
    let mut r = PixelStoreRegistry::new();
    r.open_document("d");
    r.add_layer("d", "L", N as u32, N as u32, vec![0u8; N * N * 4])
        .unwrap();

    let empty_digest = layer_digest(&r.get_layer("d", "L").unwrap().pixels, N, N);

    let mut patch = vec![0u8; 16 * 16 * 4];
    for (i, b) in patch.iter_mut().enumerate() {
        *b = ((i * 31 + 7) % 251) as u8;
    }
    r.write_region("d", "L", 8, 8, 16, 16, patch).unwrap();

    let d1 = layer_digest(&r.get_layer("d", "L").unwrap().pixels, N, N);
    assert_ne!(
        d1, empty_digest,
        "write_region must actually change the pixel digest, else D1==D2 is vacuous"
    );

    r.undo_pixel("d");
    let d_undo = layer_digest(&r.get_layer("d", "L").unwrap().pixels, N, N);
    assert_eq!(
        d_undo, empty_digest,
        "undo must restore the exact pre-patch pixels"
    );

    r.redo_pixel("d");
    let d2 = layer_digest(&r.get_layer("d", "L").unwrap().pixels, N, N);
    assert_eq!(
        d2, d1,
        "undo->redo must rebuild the byte-identical pixel digest (D2 == D1)"
    );
}
