//! Brush rasterizer for the live paint path and for the byte-for-byte paint
//! checks.
//! It owns no document state: callers keep the pixel store, this module only
//! turns dabs into tile patches and digests.
//!
//! Digest pipeline contract, pinned EXACTLY:
//!   key = "tx,ty"; combined = sorted("key:fnv1a(bytes);"...) ; digest = fnv1a(utf8(combined))

/// FNV-1a (32-bit) — must match the recorded digest bit-for-bit.
pub fn fnv1a(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in bytes {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// FNV-1a over a tile's unpremultiplied bytes, without materialising them.
/// Same byte stream as `unpremul` row-major, so the digest is identical to
/// hashing a built patch; used where the caller does not need the bytes.
fn fnv1a_unpremul_tile(buf: &[u8], x0: i64, y0: i64, tw: usize, th: usize, w: usize) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for row in 0..th as i64 {
        for col in 0..tw as i64 {
            let i = ((y0 + row) as usize * w + (x0 + col) as usize) * 4;
            for b in unpremul(buf, i) {
                h ^= b as u32;
                h = h.wrapping_mul(0x0100_0193);
            }
        }
    }
    h
}

fn mul_div_255(c: u64, a: u64) -> u8 {
    ((c * a + 127) / 255) as u8
}

#[derive(Clone, Copy, Debug)]
pub struct RasterDab {
    pub x: f64,
    pub y: f64,
    pub alpha: f64,
}

/// Straight (non-premultiplied) RGBA tip bitmap — produced by the brush-tip cache.
pub struct RasterTip<'a> {
    pub width: usize,
    pub height: usize,
    pub data: &'a [u8],
}

#[derive(Debug, Clone)]
pub struct TilePatchOut {
    pub key: String,
    pub x: i64,
    pub y: i64,
    pub w: usize,
    pub h: usize,

    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct ShadowMeta {
    pub digest: String,
    pub tile_count: usize,
    pub changed_bytes: usize,
    pub prep_us: u128,
    pub raster_us: u128,
    pub patchgen_us: u128,
    /// Sorted-by-key "(key:hash)" pairs, so a caller can diff tile changes
    /// without shipping bytes.
    pub tile_hashes: Vec<(String, String)>,
}

const TILE: i64 = 256;

pub struct TileRef {
    pub x: i64,
    pub y: i64,
    pub w: usize,
    pub h: usize,
    pub key: String,
}

pub fn tiles_in_rect(x0: i64, y0: i64, x1: i64, y1: i64, w: i64, h: i64) -> Vec<TileRef> {
    // Contract: cx0..cx1 inclusive, edge tiles clamped.
    let cx0 = 0i64.max(x0.div_euclid(TILE));
    let cy0 = 0i64.max(y0.div_euclid(TILE));
    let max_tx = (((w as f64) / TILE as f64).ceil() as i64 - 1).max(0);
    let max_ty = (((h as f64) / TILE as f64).ceil() as i64 - 1).max(0);
    let cx1 = max_tx.min((x1 - 1).div_euclid(TILE));
    let cy1 = max_ty.min((y1 - 1).div_euclid(TILE));
    let mut out = Vec::new();
    let mut ty = cy0;
    while ty <= cy1 {
        let mut tx = cx0;
        while tx <= cx1 {
            let x = tx * TILE;
            let y = ty * TILE;
            out.push(TileRef {
                x,
                y,
                w: (TILE.min(w - x)).max(0) as usize,
                h: (TILE.min(h - y)).max(0) as usize,
                key: format!("{tx},{ty}"),
            });
            tx += 1;
        }
        ty += 1;
    }
    out
}

fn unpremul(buf: &[u8], i: usize) -> [u8; 4] {
    let a = buf[i + 3] as u32;
    if a == 0 {
        return [0, 0, 0, 0];
    }
    let half = a / 2;
    [
        ((buf[i] as u32 * 255 + half) / a) as u8,
        ((buf[i + 1] as u32 * 255 + half) / a) as u8,
        ((buf[i + 2] as u32 * 255 + half) / a) as u8,
        buf[i + 3],
    ]
}

/// Rasterize the dab batch onto a fresh buffer and extract candidate tile patches.
/// `include_tiles` ships full bytes (diagnostics / small cases only).
/// `base`: when `Some`, the raster composites the dab batch onto these EXISTING
/// pixels (the canonical-commit path). When `None`, the buffer is initialized from
/// `prep_white` (white) or the blue sentinel (diagnostic shadow raster).
#[allow(clippy::too_many_arguments)] // flat args mirror the shadow-raster command struct
pub fn raster_shadow(
    w: usize,
    h: usize,
    prep_white: bool,
    base: Option<Vec<u8>>,
    eraser: bool,
    brush: f64,
    dabs: &[RasterDab],
    tip: &RasterTip,
    include_tiles: bool,
) -> (ShadowMeta, Vec<TilePatchOut>) {
    let t0 = std::time::Instant::now();
    let mut buf = match base {
        Some(b) => b,
        None => {
            let mut b = vec![0u8; w * h * 4];
            for i in 0..(w * h) {
                let (r, g, bcol) = if prep_white {
                    (255u8, 255u8, 255u8)
                } else {
                    (30u8, 120u8, 220u8)
                };
                b[i * 4] = r;
                b[i * 4 + 1] = g;
                b[i * 4 + 2] = bcol;
                b[i * 4 + 3] = 255;
            }
            b
        }
    };
    let prep_us = t0.elapsed().as_micros();

    // ── Raster (premultiplied src-over / destination-out, integer fixed-point) ──
    let t1 = std::time::Instant::now();
    let r = brush / 2.0;
    let iw = tip.width;
    let ih = tip.height;
    let stamp = brush.max(f64::MIN_POSITIVE);
    for d in dabs {
        let fx = d.x - r;
        let fy = d.y - r;
        let ox = fx.round() as i64;
        let oy = fy.round() as i64;
        let px0 = ox.clamp(0, w as i64);
        let py0 = oy.clamp(0, h as i64);
        let px1 = (ox + brush as i64).clamp(0, w as i64);
        let py1 = (oy + brush as i64).clamp(0, h as i64);
        // The stroke settings (opacity x flow) ride the dab, so the tip mask
        // stays a pure geometry/colour cache. Constant for the whole dab.
        let dab_alpha = (d.alpha.clamp(0.0, 1.0) * 255.0).round() as u64;
        // Tip data spans the full nominal diameter; map the stamp around the
        // dab centre onto it. Sampling the raw rounded offset would read only
        // the top-left corner whenever the data cap (256) is smaller than the
        // brush, and a small brush would read a transparent edge.
        //
        // The column mapping is identical to the per-pixel expression it
        // replaces, hoisted out of the inner loop: two f64 divides plus a floor
        // per pixel dominated the raster phase of a large stroke.
        let tip_xs: Vec<u32> = (px0..px1)
            .map(|px| {
                ((px as f64 + 0.5 - fx) * iw as f64 / stamp)
                    .floor()
                    .clamp(0.0, iw.saturating_sub(1) as f64) as u32
            })
            .collect();
        for py in py0..py1 {
            let tip_y = ((py as f64 + 0.5 - fy) * ih as f64 / stamp)
                .floor()
                .clamp(0.0, ih.saturating_sub(1) as f64) as usize;
            let row_base = tip_y * iw;
            let dst_row = py as usize * w;
            for px in px0..px1 {
                let ti = (row_base + tip_xs[(px - px0) as usize] as usize) * 4;
                let sa = mul_div_255(tip.data[ti + 3] as u64, dab_alpha) as u64;
                let di = (dst_row + px as usize) * 4;
                let inv = 255 - sa;
                if eraser {
                    for ch in 0..4usize {
                        buf[di + ch] = mul_div_255(buf[di + ch] as u64, inv);
                    }
                } else if sa > 0 {
                    for ch in 0..3usize {
                        let sc = mul_div_255(tip.data[ti + ch] as u64, sa) as u64;
                        let dc = buf[di + ch] as u64;
                        buf[di + ch] = (sc + mul_div_255(dc, inv) as u64).min(255) as u8;
                    }
                    buf[di + 3] = (sa + mul_div_255(buf[di + 3] as u64, inv) as u64).min(255) as u8;
                }
            }
        }
    }
    let raster_us = t1.elapsed().as_micros();

    // ── Dirty bbox + tile extraction (mirrors harness margins exactly) ──
    let t2 = std::time::Instant::now();
    let (mut minx, mut miny, mut maxx, mut maxy) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for d in dabs {
        minx = minx.min(d.x);
        miny = miny.min(d.y);
        maxx = maxx.max(d.x);
        maxy = maxy.max(d.y);
    }
    let margin = (brush / 2.0).ceil() as i64 + 2;
    let x0 = 0i64.max((minx.floor() as i64) - margin);
    let y0 = 0i64.max((miny.floor() as i64) - margin);
    let x1 = (w as i64).min((maxx.ceil() as i64) + margin);
    let y1 = (h as i64).min((maxy.ceil() as i64) + margin);

    // Only the shipped case materialises tile bytes: the commit path consumes
    // them, a digest-only run hashes the same byte stream in place. Both paths
    // previously paid a per-tile allocation plus a second full copy into
    // `out_tiles`, which at a large brush is tens of megabytes per stroke.
    let mut tile_hashes: Vec<(String, String)> = Vec::new();
    let mut tile_patches: Vec<(String, i64, i64, usize, usize, Vec<u8>)> = Vec::new();
    let mut changed_bytes: usize = 0;
    for tref in tiles_in_rect(x0, y0, x1, y1, w as i64, h as i64) {
        let bytes = tref.w * tref.h * 4;
        changed_bytes += bytes;
        if include_tiles {
            let mut data = Vec::with_capacity(bytes);
            for row in 0..tref.h as i64 {
                for col in 0..tref.w as i64 {
                    let i = ((tref.y + row) * w as i64 + (tref.x + col)) as usize * 4;
                    data.extend_from_slice(&unpremul(&buf, i));
                }
            }
            tile_hashes.push((tref.key.clone(), format!("{:08x}", fnv1a(&data))));
            tile_patches.push((tref.key, tref.x, tref.y, tref.w, tref.h, data));
        } else {
            let hash = fnv1a_unpremul_tile(&buf, tref.x, tref.y, tref.w, tref.h, w);
            tile_hashes.push((tref.key, format!("{hash:08x}")));
        }
    }
    tile_hashes.sort_by(|a, b| a.0.cmp(&b.0));
    let combined: String = tile_hashes
        .iter()
        .map(|(k, h)| format!("{k}:{h};"))
        .collect();
    let digest = format!("{:08x}", fnv1a(combined.as_bytes()));
    let patchgen_us = t2.elapsed().as_micros();

    let out_tiles: Vec<TilePatchOut> = tile_patches
        .into_iter()
        .map(|(key, x, y, w, h, data)| TilePatchOut {
            key,
            x,
            y,
            w,
            h,
            data,
        })
        .collect();

    (
        ShadowMeta {
            digest,
            tile_count: tile_hashes.len(),
            changed_bytes,
            prep_us,
            raster_us,
            patchgen_us,
            tile_hashes,
        },
        out_tiles,
    )
}

/// Deterministic re-run returning ONLY the requested keys' patches (diff diagnostics).
#[allow(clippy::too_many_arguments)] // flat args mirror the tile-key query command struct
pub fn tiles_for_keys(
    w: usize,
    h: usize,
    prep_white: bool,
    eraser: bool,
    brush: f64,
    dabs: &[RasterDab],
    tip: &RasterTip,
    keys: &[String],
) -> Vec<TilePatchOut> {
    let (_, all) = raster_shadow(w, h, prep_white, None, eraser, brush, dabs, tip, true);
    all.into_iter().filter(|t| keys.contains(&t.key)).collect()
}
