//! Paint the tool icon cursors from the embedded `tool_cursors.curres` bitmaps.

use std::sync::{Arc, OnceLock};

use gpui_kit::{point, px, size, Bounds, Corners, Pixels, Point, RenderImage, Window};

const RECORD_LEN: usize = 4268;
const RESOURCE: &[u8] = include_bytes!("../../../assets/cursors/tool_cursors.curres");
const HOTSPOTS: [(u16, u16); 3] = [(4, 28), (24, 16), (9, 11)];

fn decode_tool_rgba(index: usize) -> Vec<u8> {
    let record = &RESOURCE[index * RECORD_LEN..(index + 1) * RECORD_LEN];
    let dib = &record[4..];
    let row_stride = 32 * 4;
    let bits = &dib[40..];
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for row in 0..32 {
        let src_row = 31 - row;
        for col in 0..32 {
            let i = src_row * row_stride + col * 4;
            let dst = (row * 32 + col) * 4;
            rgba[dst] = bits[i + 2];
            rgba[dst + 1] = bits[i + 1];
            rgba[dst + 2] = bits[i];
            rgba[dst + 3] = bits[i + 3];
        }
    }
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    rgba
}

fn tool_image(index: usize) -> Arc<RenderImage> {
    static CACHE: [OnceLock<Arc<RenderImage>>; 3] =
        [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    CACHE[index]
        .get_or_init(|| {
            let rgba = decode_tool_rgba(index);
            let buffer =
                ::image::ImageBuffer::from_raw(32, 32, rgba).expect("tool cursor bitmap is 32x32");
            Arc::new(RenderImage::new(vec![::image::Frame::new(buffer)]))
        })
        .clone()
}

fn hotspot_for(index: u8) -> (f32, f32) {
    let idx = usize::from(index.min(2));
    let (x, y) = HOTSPOTS[idx];
    (f32::from(x), f32::from(y))
}

/// Paint a 32px tool cursor sprite with its cur file hotspot at `position`.
pub(super) fn painted_tool_cursor(window: &mut Window, position: (f32, f32), index: u8) {
    let idx = usize::from(index.min(2));
    let image = tool_image(idx);
    let (hx, hy) = hotspot_for(index);
    let scale = window.scale_factor();
    let edge = 32.0 * scale;
    let origin = point(px(position.0 - hx * scale), px(position.1 - hy * scale));
    let destination = Bounds {
        origin,
        size: size(px(edge), px(edge)),
    };
    let source = Bounds {
        origin: Point::<Pixels>::default(),
        size: size(px(32.), px(32.)),
    };
    let _ = window.paint_image(destination, source, Corners::default(), image, 0, false);
}
