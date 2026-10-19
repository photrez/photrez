//! PNG loading and saving for the parity tooling, normalised to 8-bit RGBA
//! with row-major top-down order. Every input (Playwright element crops,
//! Windows screen captures) is converted on load; writers always emit RGBA8.

use std::path::Path;

use super::Rect;

/// A packed RGBA8 image. Invariant: `data.len() == width * height * 4`.
#[derive(Clone)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl RgbaImage {
    /// Wrap an existing RGBA8 buffer, validating the length.
    pub fn from_rgba(width: u32, height: u32, data: Vec<u8>) -> Result<Self, String> {
        let expect = width as usize * height as usize * 4;
        if data.len() != expect {
            return Err(format!(
                "buffer is {} bytes, expected {expect} for {width}x{height} RGBA",
                data.len()
            ));
        }
        Ok(Self {
            width,
            height,
            data,
        })
    }

    /// Load a PNG, normalising to RGBA8 (palette / grayscale / RGB inputs are
    /// expanded; 16-bit inputs are scaled down to 8-bit).
    pub fn load(path: &Path) -> Result<Self, String> {
        let file =
            std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
        let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder
            .read_info()
            .map_err(|e| format!("decode {}: {e}", path.display()))?;
        let size = reader
            .output_buffer_size()
            .ok_or_else(|| format!("no image data in {}", path.display()))?;
        let mut buf = vec![0; size];
        let info = reader
            .next_frame(&mut buf)
            .map_err(|e| format!("decode {}: {e}", path.display()))?;
        let bytes = &buf[..info.buffer_size()];
        let data = match info.color_type {
            png::ColorType::Rgba => bytes.to_vec(),
            png::ColorType::Rgb => bytes
                .chunks_exact(3)
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            png::ColorType::Grayscale => bytes.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::GrayscaleAlpha => bytes
                .chunks_exact(2)
                .flat_map(|p| [p[0], p[0], p[0], p[1]])
                .collect(),
            other => {
                return Err(format!(
                    "unsupported colour type {other:?} in {}",
                    path.display()
                ))
            }
        };
        Self::from_rgba(info.width, info.height, data)
    }

    /// Write the image as RGBA8 PNG. The parity reports and side-by-side
    /// panels are artifacts; lossless output is the point.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create {}: {e}", parent.display()))?;
        }
        let file =
            std::fs::File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("encode {}: {e}", path.display()))?;
        writer
            .write_image_data(&self.data)
            .map_err(|e| format!("encode {}: {e}", path.display()))?;
        writer
            .finish()
            .map_err(|e| format!("encode {}: {e}", path.display()))
    }

    /// One pixel as `[r, g, b, a]`. Panics on out-of-range coordinates; every
    /// caller works inside validated rects.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        assert!(
            x < self.width && y < self.height,
            "pixel ({x},{y}) out of bounds"
        );
        let i = ((y as usize) * (self.width as usize) + x as usize) * 4;
        [
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        ]
    }

    /// Copy a sub-rectangle out. `rect` must lie inside the image.
    pub fn crop(&self, rect: Rect) -> Result<Self, String> {
        let (x0, y0) = (rect.x, rect.y);
        let x1 = x0.checked_add(rect.width);
        let y1 = y0.checked_add(rect.height);
        match (x1, y1) {
            (Some(x1), Some(y1)) if x1 <= self.width && y1 <= self.height => {}
            _ => {
                return Err(format!(
                    "crop {rect:?} does not fit inside {}x{}",
                    self.width, self.height
                ))
            }
        }
        let row_bytes = rect.width as usize * 4;
        let mut data = Vec::with_capacity(row_bytes * rect.height as usize);
        for y in y0..y1.expect("bounds checked above") {
            let start = ((y as usize) * (self.width as usize) + x0 as usize) * 4;
            data.extend_from_slice(&self.data[start..start + row_bytes]);
        }
        Self::from_rgba(rect.width, rect.height, data)
    }
}
