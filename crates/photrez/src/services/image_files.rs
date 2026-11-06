//! Image import and export at the file boundary.
//!
//! Owns: format selection from the chosen extension, bounded reads, dimension
//! checks before allocation, and the core codec calls.
//!
//! Must not own: document/layer models, compositing, or UI. The caller supplies
//! an already-composited RGBA buffer to export.

use std::path::Path;

use photrez_core::pixel::export::{
    decode_image_into, encode_image, image_dimensions, DecodedImageMeta,
};

use crate::services::error::ServiceError;
use crate::services::file_io::FileIo;
use crate::services::ptz_limits::{self, MAX_IMAGE_FILE_BYTES};

/// A decoded image ready to become a one-layer document.
#[derive(Debug)]
pub(crate) struct DecodedImport {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageFormat {
    Png,
    Jpeg,
    WebP,
    Bmp,
    Tiff,
}

impl ImageFormat {
    /// Open Image accepts the 0.1 picker's raster set; `.ptz` opens through
    /// the project loader instead of this service.
    pub(crate) fn from_import_path(path: &Path) -> Result<Self, ServiceError> {
        match extension(path)?.as_str() {
            "png" => Ok(Self::Png),
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            "webp" => Ok(Self::WebP),
            "bmp" => Ok(Self::Bmp),
            other => Err(ServiceError::Validation(format!(
                "unsupported import format: .{other}"
            ))),
        }
    }

    /// Export writes the 0.1 export dialog's formats. BMP is import-only.
    pub(crate) fn from_export_path(path: &Path) -> Result<Self, ServiceError> {
        match extension(path)?.as_str() {
            "png" => Ok(Self::Png),
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            "webp" => Ok(Self::WebP),
            "tiff" | "tif" => Ok(Self::Tiff),
            other => Err(ServiceError::Validation(format!(
                "unsupported export format: .{other}"
            ))),
        }
    }

    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
            Self::Bmp => "bmp",
            Self::Tiff => "tiff",
        }
    }

    fn image_format(self) -> image::ImageFormat {
        match self {
            Self::Png => image::ImageFormat::Png,
            Self::Jpeg => image::ImageFormat::Jpeg,
            Self::WebP => image::ImageFormat::WebP,
            Self::Bmp => image::ImageFormat::Bmp,
            Self::Tiff => image::ImageFormat::Tiff,
        }
    }
}

fn extension(path: &Path) -> Result<String, ServiceError> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .filter(|extension| !extension.is_empty())
        .ok_or_else(|| ServiceError::Validation("image path has no extension".into()))
}

/// Read and decode one approved image.
///
/// The path is approved by the same boundary that guards project reads. The
/// header probe runs before the pixel buffer is allocated, and the product's
/// per-side and decoded-byte budgets are enforced before the full decode.
pub(crate) fn decode_import(
    file_io: &dyn FileIo,
    path: &Path,
) -> Result<DecodedImport, ServiceError> {
    let format = ImageFormat::from_import_path(path)?;
    file_io.approve_path(path)?;
    let bytes = file_io.read_approved(path, MAX_IMAGE_FILE_BYTES)?;
    let meta = image_dimensions(&bytes, format.image_format())
        .map_err(|error| ServiceError::Validation(format!("image probe failed: {error}")))?;
    let dimensions = check_dimensions(&meta)?;
    let mut rgba = vec![0; dimensions];
    let decoded = decode_image_into(&bytes, format.image_format(), &mut rgba)
        .map_err(|error| ServiceError::Validation(format!("image decode failed: {error}")))?;
    if (decoded.width(), decoded.height()) != (meta.width(), meta.height()) {
        return Err(ServiceError::Validation(
            "decoded image dimensions changed after the probe".into(),
        ));
    }
    Ok(DecodedImport {
        width: meta.width(),
        height: meta.height(),
        rgba,
    })
}

fn check_dimensions(meta: &DecodedImageMeta) -> Result<usize, ServiceError> {
    let bytes = ptz_limits::decoded_rgba_bytes(u64::from(meta.width()), u64::from(meta.height()))?;
    usize::try_from(bytes).map_err(|_| {
        ServiceError::ResourceLimit("decoded image size does not fit in memory".into())
    })
}

/// Encode an already-composited RGBA buffer. JPEG is composited over white by
/// the core encoder; WebP is the core's lossless WebP path (quality ignored).
pub(crate) fn encode_export(
    width: u32,
    height: u32,
    rgba: &[u8],
    format: ImageFormat,
    quality: u8,
) -> Result<Vec<u8>, ServiceError> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| ServiceError::ResourceLimit("export size overflow".into()))?;
    if rgba.len() != expected {
        return Err(ServiceError::Validation(
            "export pixels do not match the document size".into(),
        ));
    }
    encode_image(width, height, rgba, format.extension(), quality)
        .map_err(|error| ServiceError::Validation(format!("image encode failed: {error}")))
}

/// Write an exported image atomically. Export paths come from the platform
/// save dialog, so they are user-authorized rather than project approvals; the
/// write still lands through a temp file and rename so a partial encode never
/// replaces an existing image.
pub(crate) fn write_export(path: &Path, bytes: &[u8]) -> Result<(), ServiceError> {
    use std::fs::{self, OpenOptions};
    use std::io::Write as _;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| ServiceError::Validation("export path has no file name".into()))?;
    let mut temp_name = name.to_os_string();
    temp_name.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let temp_path = parent.join(temp_name);

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(ServiceError::Io)?;
    let write_result = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(ServiceError::Io(error));
    }
    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(ServiceError::Io(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::services::file_io::LocalFileIo;
    use crate::services::image_files::{decode_import, encode_export, write_export, ImageFormat};
    use crate::services::ptz_limits::MAX_LAYER_DIMENSION;

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("photrez-image-files-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("test directory is created");
            // Link-free spelling: the file boundary rejects a path behind a link.
            Self(fs::canonicalize(&path).unwrap_or(path))
        }

        fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, bytes).expect("test image is written");
            path
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn file_io(directory: &Path) -> LocalFileIo {
        LocalFileIo::new(directory, directory.join("approvals.json"))
            .expect("test file boundary opens")
    }

    const RGBA: [u8; 16] = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 128,
    ];

    #[test]
    fn png_round_trips_exactly_and_jpeg_composites_on_white() {
        let directory = TestDir::new();
        let files = file_io(directory.path());
        let png = encode_export(2, 2, &RGBA, ImageFormat::Png, 100).expect("PNG encodes");
        let png_path = directory.write("source.png", &png);
        let decoded = decode_import(&files, &png_path).expect("PNG decodes");
        assert_eq!((decoded.width, decoded.height), (2, 2));
        assert_eq!(decoded.rgba, RGBA);

        let jpeg = encode_export(2, 2, &RGBA, ImageFormat::Jpeg, 95).expect("JPEG encodes");
        let jpeg_path = directory.write("source.jpg", &jpeg);
        let decoded = decode_import(&files, &jpeg_path).expect("JPEG decodes");
        assert_eq!((decoded.width, decoded.height), (2, 2));
        assert_eq!(decoded.rgba[3], 255, "JPEG has no alpha");
        let transparent_pixel = &decoded.rgba[12..16];
        assert!(
            transparent_pixel[0] > 240 && transparent_pixel[1] > 240 && transparent_pixel[2] > 240,
            "the transparent pixel must composite on white: {transparent_pixel:?}"
        );
    }

    #[test]
    fn tiff_and_webp_round_trip_the_exact_pixels() {
        use photrez_core::pixel::export::{decode_image_into, image_dimensions};

        let directory = TestDir::new();
        let files = file_io(directory.path());

        let webp = encode_export(2, 2, &RGBA, ImageFormat::WebP, 100).expect("WebP encodes");
        let webp_path = directory.write("source.webp", &webp);
        let decoded = decode_import(&files, &webp_path).expect("WebP decodes");
        assert_eq!(decoded.rgba, RGBA, "WebP export is lossless");

        // TIFF is export-only; prove the artifact decodes with the core codec.
        let tiff = encode_export(2, 2, &RGBA, ImageFormat::Tiff, 100).expect("TIFF encodes");
        let meta = image_dimensions(&tiff, image::ImageFormat::Tiff).expect("TIFF probes");
        assert_eq!((meta.width(), meta.height()), (2, 2));
        let mut decoded = vec![0; 2 * 2 * 4];
        decode_image_into(&tiff, image::ImageFormat::Tiff, &mut decoded).expect("TIFF decodes");
        assert_eq!(decoded, RGBA, "TIFF export is lossless");
    }

    #[test]
    fn unsupported_extensions_and_oversized_dimensions_are_rejected() {
        let directory = TestDir::new();
        let files = file_io(directory.path());
        let gif = directory.write("source.gif", b"GIF89a");
        let error = decode_import(&files, &gif).expect_err("GIF is not an import format");
        assert_eq!(error.code(), "E_VALIDATION");

        let wide = encode_export(
            MAX_LAYER_DIMENSION as u32 + 1,
            1,
            &vec![0; (MAX_LAYER_DIMENSION as usize + 1) * 4],
            ImageFormat::Png,
            100,
        )
        .expect("wide PNG encodes");
        let wide_path = directory.write("wide.png", &wide);
        let error = decode_import(&files, &wide_path).expect_err("oversized image is rejected");
        assert_eq!(error.code(), "E_VALIDATION", "{error}");
    }

    #[test]
    fn export_length_mismatch_is_rejected_before_encoding() {
        let error = encode_export(2, 2, &RGBA[..8], ImageFormat::Png, 100)
            .expect_err("short buffer is rejected");
        assert_eq!(error.code(), "E_VALIDATION");
    }

    #[test]
    fn unsupported_export_extension_is_rejected() {
        for path in ["out.gif", "out.bmp", "out"] {
            let error = ImageFormat::from_export_path(Path::new(path))
                .expect_err("unsupported export extension is rejected");
            assert_eq!(error.code(), "E_VALIDATION", "{path}");
        }
        for path in [
            "out.png", "out.jpg", "out.jpeg", "out.webp", "out.tiff", "out.tif",
        ] {
            assert!(
                ImageFormat::from_export_path(Path::new(path)).is_ok(),
                "{path}"
            );
        }
    }

    #[test]
    fn missing_import_file_is_io_and_recovers_after_creation() {
        let directory = TestDir::new();
        let files = file_io(directory.path());
        let path = directory.path().join("late.png");
        let error = decode_import(&files, &path).expect_err("missing file is refused");
        assert_eq!(error.code(), "E_IO", "{error}");

        let png = encode_export(2, 2, &RGBA, ImageFormat::Png, 100).expect("PNG encodes");
        fs::write(&path, png).expect("file is created");
        let decoded = decode_import(&files, &path).expect("the valid file succeeds");
        assert_eq!(decoded.rgba, RGBA);
    }

    #[test]
    fn export_write_is_atomic_and_leaves_no_temp_file() {
        let directory = TestDir::new();
        let path = directory.path().join("out.png");
        let bytes = encode_export(2, 2, &RGBA, ImageFormat::Png, 100).expect("PNG encodes");
        write_export(&path, &bytes).expect("export writes");
        assert_eq!(fs::read(&path).expect("export reads back"), bytes);
        assert_eq!(
            fs::read_dir(directory.path())
                .expect("directory reads")
                .count(),
            1,
            "no temporary file remains"
        );

        let missing_parent = directory.path().join("missing").join("out.png");
        let error = write_export(&missing_parent, &bytes).expect_err("missing parent is refused");
        assert_eq!(error.code(), "E_IO");
        assert!(!missing_parent.exists());
    }
}
