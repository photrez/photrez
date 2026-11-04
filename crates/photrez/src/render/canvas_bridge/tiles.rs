#[cfg(test)]
use super::TILE_SIZE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TileRect {
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

#[cfg(test)]
pub(super) fn tile_rects(width: u32, height: u32) -> Vec<TileRect> {
    tile_rects_with_size(width, height, TILE_SIZE)
}

pub(super) fn tile_rects_with_size(width: u32, height: u32, tile_size: u32) -> Vec<TileRect> {
    debug_assert!(tile_size > 0, "tile size is validated by the constructor");
    let cols = width.div_ceil(tile_size) as usize;
    let rows = height.div_ceil(tile_size) as usize;
    let mut rects = Vec::with_capacity(cols.saturating_mul(rows));
    let mut y = 0;
    while y < height {
        let tile_height = (height - y).min(tile_size);
        let mut x = 0;
        while x < width {
            let tile_width = (width - x).min(tile_size);
            rects.push(TileRect {
                x,
                y,
                width: tile_width,
                height: tile_height,
            });
            x += tile_width;
        }
        y += tile_height;
    }
    rects
}

/// Smallest rectangle covering both inputs. Coordinates are document pixels;
/// saturation keeps a malformed rect from wrapping, while normal callers pass
/// rects already inside the canvas.
pub fn union_rect(a: (u32, u32, u32, u32), b: (u32, u32, u32, u32)) -> (u32, u32, u32, u32) {
    let ax1 = a.0.saturating_add(a.2);
    let ay1 = a.1.saturating_add(a.3);
    let bx1 = b.0.saturating_add(b.2);
    let by1 = b.1.saturating_add(b.3);
    let x0 = a.0.min(b.0);
    let y0 = a.1.min(b.1);
    let x1 = ax1.max(bx1);
    let y1 = ay1.max(by1);
    (x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
}
