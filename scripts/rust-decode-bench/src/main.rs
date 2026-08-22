// R5 re-verification: fair decode benchmark (docs/plans/2026-08-21-rust-gpu-benchmark-matrix.md).
// Contenders for the SAME bytes:
//   - image 0.25 load_from_memory   (the OLD bench's technique)
//   - zune-png / zune-jpeg          (modern SIMD-accelerated decoders)
// Fixtures are generated in-process (noise+gradient mix = worst-case inflate),
// written once to temp, then decoded N times; median reported.
// Run: cargo run --release   (from this dir, with RUSTFLAGS=-C target-cpu=native)

use std::hint::black_box;
use std::io::Cursor;
use std::time::Instant;

fn make_rgba(w: usize, h: usize) -> Vec<u8> {
    let mut v = vec![0u8; w * h * 4];
    for i in 0..w * h {
        let x = i % w;
        let y = i / w;
        // noise + gradient mix: incompressible enough to be a fair inflate test
        v[i * 4] = ((x * 7) ^ (y * 13)) as u8;
        v[i * 4 + 1] = ((x * 11 + y * 3) & 0xff) as u8;
        v[i * 4 + 2] = (((x * x + y * y) >> 6) & 0xff) as u8;
        v[i * 4 + 3] = 255;
    }
    v
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

fn bench<F: FnMut()>(label: &str, mp: f64, iters: usize, mut f: F) -> f64 {
    f(); // warmup
    let mut ts = Vec::with_capacity(iters);
    for _ in 0..iters {
        let s = Instant::now();
        f();
        ts.push(s.elapsed().as_secs_f64() * 1e3);
    }
    let m = median(ts);
    println!("  {:<40} med={:>8.2}ms  ({:>5.0} MP/s)", label, m, mp / (m / 1e3));
    m
}

fn checksum(px: &[u8]) -> u64 {
    px.iter().step_by(1024).map(|&b| b as u64).sum()
}

fn main() {
    let dir = std::env::temp_dir().join("photrez-decode-bench");
    std::fs::create_dir_all(&dir).unwrap();
    println!("fixtures dir: {}", dir.display());
    println!("opt-level note: run with --release; RUSTFLAGS=-C target-cpu=native recommended\n");

    for (name, w, h) in [("1080p", 1920usize, 1080usize), ("4K", 4000, 3000)] {
        let mp = (w * h) as f64 / 1e6;
        println!("== {} ({}x{}, {:.1} MP) ==", name, w, h, mp);
        let rgba = make_rgba(w, h);

        // encode fixtures once
        let png_bytes = {
            let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba.clone()).unwrap();
            let mut out = Vec::new();
            image::DynamicImage::ImageRgba8(img)
                .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
                .unwrap();
            out
        };
        let jpg_bytes = {
            let mut rgb = Vec::with_capacity(w * h * 3);
            for p in rgba.chunks_exact(4) {
                rgb.extend_from_slice(&p[0..3]);
            }
            let img = image::RgbImage::from_raw(w as u32, h as u32, rgb).unwrap();
            let mut out = Vec::new();
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90);
            image::DynamicImage::ImageRgb8(img)
                .write_with_encoder(enc)
                .unwrap();
            out
        };
        println!(
            "  fixture sizes: PNG {:.1}MB, JPEG {:.1}MB",
            png_bytes.len() as f64 / 1e6,
            jpg_bytes.len() as f64 / 1e6
        );

        let png_path = dir.join(format!("test-{}.png", name));
        let jpg_path = dir.join(format!("test-{}.jpg", name));
        std::fs::write(&png_path, &png_bytes).unwrap();
        std::fs::write(&jpg_path, &jpg_bytes).unwrap();

        // PNG contenders
        let iters = if w > 2000 { 3 } else { 6 };
        let m_img_png = bench("PNG via image::load_from_memory", mp, iters, || {
            let img = black_box(image::load_from_memory(black_box(&png_bytes)).unwrap()).to_rgba8();
            black_box(checksum(img.as_raw()));
        });
        let m_zpng = bench("PNG via zune-png decode()", mp, iters, || {
            let mut d = zune_png::PngDecoder::new(zune_core::bytestream::ZCursor::new(&png_bytes));
            let px = black_box(d.decode().unwrap());
            let sum = match px {
                zune_core::result::DecodingResult::U8(v) => checksum(&v),
                _ => panic!("expected u8"),
            };
            black_box(sum);
        });

        // JPEG contenders
        let m_img_jpg = bench("JPEG via image::load_from_memory", mp, iters, || {
            let img = black_box(image::load_from_memory(black_box(&jpg_bytes)).unwrap()).to_rgba8();
            black_box(checksum(img.as_raw()));
        });
        let m_zjpg = bench("JPEG via zune-jpeg decode()", mp, iters, || {
            let mut d = zune_jpeg::JpegDecoder::new(zune_core::bytestream::ZCursor::new(&jpg_bytes));
            let px = black_box(d.decode().unwrap());
            black_box(checksum(&px));
        });

        println!(
            "  -> zune speedup vs image-crate: PNG {:.2}x, JPEG {:.2}x",
            m_img_png / m_zpng,
            m_img_jpg / m_zjpg
        );
        println!();
    }
}
