/// Clipped bounding box of one disc, or `None` when the disc misses the page.
/// The paint loop and the dirty upload use the same bounds.
pub(crate) fn disc_bounds(
    center_x: i32,
    center_y: i32,
    radius: i32,
    width: u32,
    height: u32,
) -> Option<(u32, u32, u32, u32)> {
    if radius < 0 || width == 0 || height == 0 {
        return None;
    }
    let x0 = (center_x - radius).max(0);
    let x1 = (center_x + radius).min(width as i32 - 1);
    let y0 = (center_y - radius).max(0);
    let y1 = (center_y + radius).min(height as i32 - 1);
    if x0 > x1 || y0 > y1 {
        return None;
    }
    Some((
        x0 as u32,
        y0 as u32,
        (x1 - x0 + 1) as u32,
        (y1 - y0 + 1) as u32,
    ))
}

/// Fixture content for one layer, `depth` rows above the bottom. The formulas
/// are the ones the input harness's generated `.ptz` uses, so both harnesses
/// measure the same three-layer document: the bottom layer is an opaque ramp,
/// every layer above it is a translucent block grid with its own cell, offset
/// and colour plus a light band.
pub(crate) fn fixture_layer_pixels(
    depth: usize,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| format!("{width}x{height} overflows the buffer length"))?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(bytes).map_err(|error| {
        format!("cannot allocate {bytes} bytes for fixture layer {depth}: {error:?}")
    })?;
    rgba.resize(bytes, 0);
    if depth == 0 {
        let last_x = width.saturating_sub(1).max(1) as usize;
        let last_y = height.saturating_sub(1).max(1) as usize;
        for y in 0..height {
            let green = (y as usize * 255 / last_y) as u8;
            let row = (y as usize * width as usize) * 4;
            for x in 0..width {
                let red = (x as usize * 255 / last_x) as u8;
                let pixel = row + (x as usize) * 4;
                rgba[pixel] = red;
                rgba[pixel + 1] = green;
                rgba[pixel + 2] = 255 - red / 2;
                rgba[pixel + 3] = 255;
            }
        }
        return Ok(rgba);
    }
    let cell = (64 + depth * 7) as u32;
    let shift = (depth * 23) as u32;
    let block_column = (depth % 3) as u32;
    let color = [
        (40 + 30 * depth).min(255) as u8,
        200usize.saturating_sub(18 * depth) as u8,
        (90 + 21 * depth).min(255) as u8,
    ];
    let band_start = height / (depth as u32 + 1);
    let band_end = band_start + (height / 16).max(1);
    for y in 0..height {
        let in_band = (y >= band_start && y < band_end) || y < 4;
        let row_block = ((y + shift) / cell) % 3 == block_column;
        let row = (y as usize * width as usize) * 4;
        for x in 0..width {
            let pixel = row + (x as usize) * 4;
            if row_block && ((x + shift * 2) / cell) % 3 == block_column {
                rgba[pixel..pixel + 4].copy_from_slice(&[color[0], color[1], color[2], 170]);
            } else if in_band {
                rgba[pixel..pixel + 4].copy_from_slice(&[255, 255, 255, 90]);
            }
        }
    }
    Ok(rgba)
}

/// Copies one row-major RGBA sub-rectangle out of a wider document buffer.
/// The caller guarantees the rectangle is inside the source.
pub(crate) fn copy_region(
    source: &[u8],
    width: u32,
    x: u32,
    y: u32,
    region_width: u32,
    region_height: u32,
) -> Vec<u8> {
    let row_len = region_width as usize * 4;
    let mut out = Vec::with_capacity(row_len * region_height as usize);
    for row in y..y + region_height {
        let start = ((row as usize * width as usize) + x as usize) * 4;
        out.extend_from_slice(&source[start..start + row_len]);
    }
    out
}

/// The build profile this measurement ran under, recorded so a timing number always
/// carries the only fact that decides whether it is comparable to another run.
pub(crate) fn build_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "dev"
    } else {
        "release"
    }
}

#[cfg(test)]
mod tests {
    use super::{copy_region, disc_bounds, fixture_layer_pixels};

    #[test]
    fn canonical_fixture_layers_are_distinct_and_composite_through() {
        // A blank document is not a measurement fixture: the bottom layer must
        // be an opaque ramp and every layer above it must differ from the
        // others while leaving transparent pixels for the composite to see.
        let (width, height) = (512u32, 256u32);
        let bottom = fixture_layer_pixels(0, width, height).expect("bottom layer");
        let middle = fixture_layer_pixels(1, width, height).expect("middle layer");
        let top = fixture_layer_pixels(2, width, height).expect("top layer");
        assert_ne!(bottom, middle);
        assert_ne!(middle, top);
        let distinct = |pixels: &[u8]| -> std::collections::HashSet<[u8; 4]> {
            pixels
                .chunks_exact(4)
                .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
                .collect()
        };
        assert!(distinct(&bottom).len() > 100, "bottom layer is degenerate");
        for (label, pixels) in [("middle", &middle), ("top", &top)] {
            let values = distinct(pixels);
            assert!(values.len() >= 3, "{label} layer is degenerate");
            assert!(
                values.contains(&[0u8, 0, 0, 0]),
                "{label} layer has no transparent pixel to composite through"
            );
        }
        assert!(bottom.chunks_exact(4).all(|pixel| pixel[3] == 255));
        assert_eq!(&bottom[0..4], &[0, 0, 255, 255]);
        assert_eq!(&bottom[(511 * 4)..512 * 4], &[255, 0, 128, 255]);
    }

    #[test]
    fn fixture_rejects_an_impossible_allocation_size() {
        assert!(fixture_layer_pixels(0, u32::MAX, u32::MAX).is_err());
    }

    #[test]
    fn disc_bounds_clips_to_the_document_and_rejects_empty() {
        assert_eq!(disc_bounds(5, 5, 2, 10, 10), Some((3, 3, 5, 5)));
        assert_eq!(disc_bounds(0, 0, 2, 10, 10), Some((0, 0, 3, 3)));
        assert_eq!(disc_bounds(9, 9, 2, 10, 10), Some((7, 7, 3, 3)));
        assert_eq!(disc_bounds(10, 5, 2, 10, 10), Some((8, 3, 2, 5)));
        assert_eq!(disc_bounds(-5, 5, 2, 10, 10), None);
        assert_eq!(disc_bounds(5, 5, 0, 10, 10), Some((5, 5, 1, 1)));
        assert_eq!(disc_bounds(5, 5, 2, 0, 10), None);
    }

    #[test]
    fn copy_region_copies_rows_with_the_requested_stride() {
        let source: Vec<u8> = (0..(3 * 2 * 4) as u8).collect();
        let copied = copy_region(&source, 3, 1, 0, 2, 2);
        assert_eq!(copied.len(), 2 * 2 * 4);
        assert_eq!(&copied[0..8], &source[4..12]);
        assert_eq!(&copied[8..16], &source[16..24]);
    }
}
