//! The measurement fixture: the documents a run loads, the pixel content of
//! their layers and the PNG/ZIP encoders that keep a generated `.ptz` byte-for
//! byte reproducible.

use std::io::Write;
use std::path::PathBuf;

use serde_json::Value;

use super::args::Config;

/// The 2x2 bounded document: the smallest real `.ptz` the loader accepts, used
/// when a run only needs the pipeline rather than a workload.
const BOUNDED_FIXTURE: &[u8] = include_bytes!("../../../../tests/fixtures/bounded-raster.ptz");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Base {
    Blank,
    Opaque,
}

impl Base {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Blank => "blank",
            Self::Opaque => "opaque",
        }
    }

    fn pixel(self) -> [u8; 4] {
        match self {
            Self::Blank => [0, 0, 0, 0],
            Self::Opaque => [255, 255, 255, 255],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Content {
    Plain,
    Canonical,
}

impl Content {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Canonical => "canonical",
        }
    }
}

/// Stages the fixture the run opens and returns its path plus the label the
/// receipt records. `--canvas 2` selects the bounded document; anything larger
/// is generated so the byte content is reproducible from the arguments alone.
pub(super) fn stage_fixture(
    root: &std::path::Path,
    config: &Config,
) -> Result<(PathBuf, String), String> {
    if config.canvas == 2 {
        let path = root.join("bounded-raster.ptz");
        std::fs::write(&path, BOUNDED_FIXTURE)
            .map_err(|error| format!("cannot stage bounded fixture: {error}"))?;
        return Ok((path, "bounded-raster.ptz".into()));
    }
    let label = format!(
        "generated-{}x{}-{}l-{}-{}.ptz",
        config.canvas,
        config.height(),
        config.layers,
        config.base.label(),
        config.content.label()
    );
    let path = root.join(&label);
    let bytes = build_ptz(
        config.canvas,
        config.height(),
        config.layers,
        config.base,
        config.content,
    )?;
    std::fs::write(&path, &bytes)
        .map_err(|error| format!("cannot stage generated fixture: {error}"))?;
    Ok((path, label))
}

fn build_ptz(
    width: u32,
    height: u32,
    layers: usize,
    base: Base,
    content: Content,
) -> Result<Vec<u8>, String> {
    if layers == 0 {
        return Err("a fixture needs at least one raster layer".to_owned());
    }
    let raster_layers: Vec<Value> = (0..layers)
        .map(|index| {
            serde_json::json!({
                "id": format!("raster-{}", index + 1),
                "name": format!("Raster {}", index + 1),
                "type": "raster",
                "visible": true,
                "opacity": 1.0,
                "locked": false,
                "blendMode": "normal",
                "transform": {
                    "x": 0.0, "y": 0.0, "scaleX": 1.0, "scaleY": 1.0,
                    "rotation": 0.0, "flipH": false, "flipV": false
                },
                "width": width,
                "height": height
            })
        })
        .collect();
    let document = serde_json::json!({
        "format": "photrez-ptz",
        "version": 3,
        "id": "measure-fixture",
        "name": format!("Generated {width}x{height} {}", base.label()),
        "width": width,
        "height": height,
        "layers": raster_layers
    });
    let document = serde_json::to_vec(&document)
        .map_err(|error| format!("cannot encode fixture document: {error}"))?;
    let cursor = std::io::Cursor::new(Vec::new());
    let mut writer = zip::write::ZipWriter::new(cursor);
    writer
        .start_file("document.json", fixture_file_options())
        .map_err(|error| format!("cannot start fixture document entry: {error}"))?;
    writer
        .write_all(&document)
        .map_err(|error| format!("cannot write fixture document: {error}"))?;
    for index in 1..=layers {
        let depth = layers - index;
        let rgba = layer_pixels(depth, width, height, base, content);
        let png = encode_png_rows(width, height, &rgba)?;
        writer
            .start_file(format!("layers/raster-{index}.png"), fixture_file_options())
            .map_err(|error| format!("cannot start fixture layer entry: {error}"))?;
        writer
            .write_all(&png)
            .map_err(|error| format!("cannot write fixture layer: {error}"))?;
    }
    let cursor = writer
        .finish()
        .map_err(|error| format!("cannot finish fixture archive: {error}"))?;
    Ok(cursor.into_inner())
}

/// Fixture content for one layer, `depth` rows above the bottom. The formulas
/// are the documented ones the `measure-frames` display-seam harness uses for
/// the same fixture, so both harnesses show the same three-layer document: the
/// bottom layer is an opaque ramp, every layer above it is a translucent block
/// grid with its own cell, offset and colour plus a light band.
fn layer_pixels(depth: usize, width: u32, height: u32, base: Base, content: Content) -> Vec<u8> {
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .unwrap_or(0);
    let mut rgba = vec![0u8; bytes];
    if content == Content::Plain {
        for pixel in rgba.chunks_exact_mut(4) {
            pixel.copy_from_slice(&base.pixel());
        }
        return rgba;
    }
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
        return rgba;
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
    rgba
}

/// Encodes one row-major RGBA buffer as an 8-bit RGBA PNG.
fn encode_png_rows(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 {
        return Err("PNG dimensions must be non-zero".to_owned());
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "PNG data size overflow".to_owned())?;
    if rgba.len() != expected {
        return Err("PNG buffer does not match its dimensions".to_owned());
    }
    let row = (width as usize)
        .checked_mul(4)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| "PNG row size overflow".to_owned())?;
    let raw_len = (height as usize)
        .checked_mul(row)
        .ok_or_else(|| "PNG data size overflow".to_owned())?;
    let mut raw = Vec::with_capacity(raw_len);
    for y in 0..height as usize {
        raw.push(0);
        let start = y * width as usize * 4;
        raw.extend_from_slice(&rgba[start..start + width as usize * 4]);
    }
    let zlib = zlib_store(&raw);
    let mut png = Vec::with_capacity(zlib.len() + 64);
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    push_png_chunk(&mut png, b"IHDR", &ihdr)?;
    push_png_chunk(&mut png, b"IDAT", &zlib)?;
    push_png_chunk(&mut png, b"IEND", &[])?;
    Ok(png)
}

fn fixture_file_options() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::DEFAULT
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6))
        .system(zip::System::Dos)
        .external_attributes(0o600 << 16)
}

fn push_png_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) -> Result<(), String> {
    let length = u32::try_from(data.len()).map_err(|_| "PNG chunk is too large".to_owned())?;
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32_chunks(&[tag, data]).to_be_bytes());
    Ok(())
}

fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 65_535 * 5 + 16);
    out.extend_from_slice(&[0x78, 0x01]);
    let mut offset = 0;
    loop {
        let remaining = data.len().saturating_sub(offset);
        let take = remaining.min(65_535);
        let end = offset + take;
        let final_block = end == data.len();
        out.push(u8::from(final_block));
        let length = take as u16;
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&(!length).to_le_bytes());
        if let Some(chunk) = data.get(offset..end) {
            out.extend_from_slice(chunk);
        }
        offset = end;
        if final_block {
            break;
        }
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65_521;
    let mut first = 1u32;
    let mut second = 0u32;
    for &byte in data {
        first = (first + byte as u32) % MOD_ADLER;
        second = (second + first) % MOD_ADLER;
    }
    (second << 16) | first
}

fn crc32_chunks(chunks: &[&[u8]]) -> u32 {
    let mut table = [0u32; 256];
    for (index, entry) in table.iter_mut().enumerate() {
        let mut value = index as u32;
        for _ in 0..8 {
            value = if value & 1 != 0 {
                0xEDB8_8320 ^ (value >> 1)
            } else {
                value >> 1
            };
        }
        *entry = value;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for chunk in chunks {
        for &byte in *chunk {
            crc = table[((crc ^ byte as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
    }
    crc ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::{adler32, build_ptz, crc32_chunks, encode_png_rows, layer_pixels, Base, Content};

    #[test]
    fn crc_and_adler_match_known_vectors() {
        assert_eq!(crc32_chunks(&[b"123456789"]), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn png_encoder_emits_signature_and_idat() {
        let png =
            encode_png_rows(2, 1, &[12, 34, 56, 255, 12, 34, 56, 255]).expect("small PNG encodes");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png.windows(4).any(|window| window == b"IDAT"));
    }

    #[test]
    fn png_encoder_refuses_a_mismatched_buffer() {
        assert_eq!(
            encode_png_rows(2, 1, &[12, 34, 56, 255]).err().as_deref(),
            Some("PNG buffer does not match its dimensions")
        );
    }

    #[test]
    fn generated_fixture_is_a_zip_archive() {
        let bytes = build_ptz(2, 2, 1, Base::Blank, Content::Plain).expect("fixture builds");
        assert!(bytes.starts_with(b"PK\x03\x04"));
        assert!(bytes.len() > 100);
    }

    #[test]
    fn generated_fixture_carries_every_requested_layer() {
        let bytes = build_ptz(8, 4, 3, Base::Opaque, Content::Plain).expect("three-layer builds");
        let text = String::from_utf8_lossy(&bytes);
        for index in 1..=3 {
            assert!(
                text.contains(&format!("layers/raster-{index}.png")),
                "layer {index} entry is present"
            );
        }
        assert!(!text.contains("layers/raster-4.png"));
    }

    #[test]
    fn plain_fixture_layers_are_uniform() {
        for depth in 0..3 {
            let pixels = layer_pixels(depth, 8, 4, Base::Opaque, Content::Plain);
            assert!(pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 255, 255, 255]));
        }
    }

    #[test]
    fn canonical_fixture_layers_are_distinct_and_non_uniform() {
        // A blank document is not a measurement fixture: each depth must differ
        // from the others and carry more than one pixel value. The cell grid is
        // sized for a document of at least a few hundred pixels per edge, which
        // is the canonical fixture's scale.
        let (width, height) = (512u32, 256u32);
        let bottom = layer_pixels(0, width, height, Base::Opaque, Content::Canonical);
        let middle = layer_pixels(1, width, height, Base::Opaque, Content::Canonical);
        let top = layer_pixels(2, width, height, Base::Opaque, Content::Canonical);
        assert_ne!(bottom, middle);
        assert_ne!(middle, top);
        let distinct = |pixels: &[u8]| -> std::collections::HashSet<[u8; 4]> {
            pixels
                .chunks_exact(4)
                .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
                .collect()
        };
        // The backdrop is a ramp, so it carries a colour per column.
        assert!(distinct(&bottom).len() > 100, "bottom layer is degenerate");
        // Each layer above adds its own block colour and band over transparency.
        for (label, pixels) in [("middle", &middle), ("top", &top)] {
            let values = distinct(pixels);
            assert!(values.len() >= 3, "{label} layer is degenerate");
            assert!(
                values.contains(&[0u8, 0, 0, 0]),
                "{label} layer has no transparent pixel to composite through"
            );
        }
        // The bottom layer is the opaque backdrop every composite rests on.
        assert!(bottom.chunks_exact(4).all(|pixel| pixel[3] == 255));
        // The ramp is what makes the fixture hash content-sensitive.
        assert_eq!(&bottom[0..4], &[0, 0, 255, 255]);
        assert_eq!(&bottom[(511 * 4)..512 * 4], &[255, 0, 128, 255]);
    }
}
