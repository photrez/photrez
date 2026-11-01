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

#[cfg(test)]
mod tests {
    use super::{copy_region, disc_bounds};

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
