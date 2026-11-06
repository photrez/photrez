use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read, Seek};
use std::path::Path;
use std::str;

use photrez_core::document::content::{
    BlendMode, LayerType, PtzDocumentContent, PtzLayer, Transform2D,
};
use photrez_core::pixel::export::{decode_png_into, png_dimensions};
use photrez_core::pixel::PixelStoreRegistry;
use photrez_core::ptz::PtzDocument;
use zip::ZipArchive;

use crate::services::error::ServiceError;
use crate::services::file_io::FileIo;
use crate::services::ptz_limits::{
    self, ArchiveBudget, MAX_ARCHIVE_ENTRY_BYTES, MAX_DOCUMENT_JSON_BYTES, MAX_LAYER_COUNT,
    MAX_LAYER_DIMENSION, MAX_STRUCTURAL_ENTRIES,
};

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const PTZ_FORMAT: &str = "photrez-ptz";
const MAX_SUPPORTED_PTZ_VERSION: u64 = 3;

/// A detached core owner with the composited display image for the display seam.
/// Only read-only views are exposed outside this crate.
pub struct LoadedDocument {
    pub(crate) registry: PixelStoreRegistry,
    pub(crate) document_id: String,
    pub(crate) display_width: u32,
    pub(crate) display_height: u32,
    pub(crate) display_rgba: Vec<u8>,
    document_snapshot: PtzDocument,
}

impl LoadedDocument {
    /// ID assigned to the loaded core document.
    pub fn document_id(&self) -> &str {
        &self.document_id
    }

    /// Typed PTZ content while the candidate's core registry retains it.
    pub fn content_snapshot(&self) -> Option<&PtzDocumentContent> {
        self.registry.document_content(&self.document_id)
    }

    /// Immutable typed snapshot of the file payload, including session fields.
    pub fn document_snapshot(&self) -> &PtzDocument {
        &self.document_snapshot
    }

    /// Record the live viewport in the session envelope. The viewport is
    /// session state, not document content: this never marks the document
    /// dirty, and a later save writes the last observed camera.
    pub(crate) fn set_viewport(&mut self, pan_x: f64, pan_y: f64, zoom: f64) {
        if !pan_x.is_finite() || !pan_y.is_finite() || !zoom.is_finite() || zoom <= 0.0 {
            return;
        }
        self.document_snapshot.viewport.pan_x = pan_x;
        self.document_snapshot.viewport.pan_y = pan_y;
        self.document_snapshot.viewport.zoom = zoom;
    }

    /// Borrow the composited display image as row-major RGBA.
    pub fn canonical_rgba(&self) -> (u32, u32, &[u8]) {
        (self.display_width, self.display_height, &self.display_rgba)
    }

    /// Build the one-layer document an imported raster becomes. The image is
    /// already decoded and bounded by the import service; this seeds the same
    /// core owner and file envelope a project open produces.
    pub(crate) fn imported_image(
        document_id: String,
        name: String,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<Self, ServiceError> {
        Self::single_layer_document(document_id, name, width, height, rgba, None)
    }

    /// The new-document Background: one white raster layer that owns the
    /// document's layer identity and carries the 0.1 build's locked position and
    /// rotation, so the Layers row and the Properties banner read real core
    /// state instead of seeded display flags.
    pub(crate) fn seeded_background(
        document_id: String,
        name: String,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<Self, ServiceError> {
        Self::single_layer_document(document_id, name, width, height, rgba, Some(true))
    }

    fn single_layer_document(
        document_id: String,
        name: String,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        locked_transform: Option<bool>,
    ) -> Result<Self, ServiceError> {
        ptz_limits::validate_layer_id(&document_id)?;
        if width == 0 || height == 0 {
            return Err(validation("imported image dimensions must be positive"));
        }
        let expected = ptz_limits::decoded_rgba_bytes(u64::from(width), u64::from(height))?;
        let expected = usize::try_from(expected)
            .map_err(|_| resource_limit("imported image size does not fit in memory"))?;
        if rgba.len() != expected {
            return Err(validation(
                "imported pixels do not match the image dimensions",
            ));
        }

        let layer = PtzLayer {
            id: "background".to_owned(),
            name: "Background".to_owned(),
            layer_type: LayerType::Raster,
            visible: true,
            opacity: 1.0,
            locked: false,
            is_background: Some(true),
            lock_transparency: None,
            lock_position: locked_transform,
            lock_rotation: locked_transform,
            has_adjustments: None,
            basic_adjustment: None,
            resource_id: None,
            blend_mode: BlendMode::Normal,
            transform: Transform2D {
                x: 0.0,
                y: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
            },
            width: f64::from(width),
            height: f64::from(height),
            shape_params: None,
            text_data: None,
        };
        let content = PtzDocumentContent {
            id: document_id.clone(),
            name: name.clone(),
            width: f64::from(width),
            height: f64::from(height),
            layers: vec![layer.clone()],
            selection: None,
        };
        let snapshot = PtzDocument {
            id: document_id.clone(),
            name,
            width: f64::from(width),
            height: f64::from(height),
            layers: vec![layer],
            active_layer_id: None,
            selection: None,
            viewport: Default::default(),
            dirty: false,
        };

        let mut registry = PixelStoreRegistry::new();
        registry.open_document(&document_id);
        registry
            .add_layer(&document_id, "background", width, height, rgba.clone())
            .map_err(|error| validation(format!("cannot seed imported pixels: {error}")))?;
        registry
            .seed_document(&document_id, content)
            .map_err(|error| validation(format!("cannot seed imported metadata: {error}")))?;
        Ok(Self {
            registry,
            document_id,
            display_width: width,
            display_height: height,
            display_rgba: rgba,
            document_snapshot: snapshot,
        })
    }
}

struct ArchiveEntry {
    index: usize,
    size: u64,
}

enum EntryName {
    Document,
    Layer(String),
}

struct RasterMetadata {
    id: String,
    width: u32,
    height: u32,
    entry: Option<ArchiveEntry>,
}

/// Load an approved archive as a detached candidate for the supported display boundary.
///
/// The display boundary accepts one visible, opaque, normal-blend raster with
/// an identity transform; unsupported content returns a validation gap.
pub fn load_document_candidate(
    file_io: &dyn FileIo,
    approved_path: &Path,
) -> Result<LoadedDocument, ServiceError> {
    let archive_bytes = file_io.read_approved(approved_path, MAX_ARCHIVE_ENTRY_BYTES)?;
    let archive_size = u64::try_from(archive_bytes.len())
        .map_err(|_| resource_limit("project archive size does not fit in file-size range"))?;
    if archive_size > MAX_ARCHIVE_ENTRY_BYTES {
        return Err(resource_limit("project archive exceeds its byte limit"));
    }

    let central_entry_count = preflight_central_directory_entry_count(&archive_bytes)?;
    let mut archive =
        ZipArchive::new(Cursor::new(archive_bytes.as_slice())).map_err(invalid_archive)?;
    if central_entry_count != archive.len() {
        return Err(validation("duplicate project archive entry names"));
    }
    if archive.has_overlapping_entries().map_err(invalid_archive)? {
        return Err(validation("project archive entries overlap"));
    }

    let mut seen_names = HashSet::with_capacity(archive.len());
    let mut document_entry: Option<ArchiveEntry> = None;
    let mut layer_entries = HashMap::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(invalid_archive)?;
        if file.encrypted() || file.is_symlink() {
            return Err(validation(
                "encrypted and symbolic-link entries are not allowed",
            ));
        }
        let raw_name = file.name_raw();
        if !raw_name.is_ascii() {
            return Err(validation("project archive entry names must be ASCII"));
        }
        let name = str::from_utf8(raw_name)
            .map_err(|error| validation(format!("invalid project archive entry name: {error}")))?;
        if !seen_names.insert(name.to_owned()) {
            return Err(validation("duplicate project archive entry"));
        }
        let entry = ArchiveEntry {
            index,
            size: file.size(),
        };
        match parse_entry_name(name)? {
            EntryName::Document => document_entry = Some(entry),
            EntryName::Layer(layer_id) => {
                layer_entries.insert(layer_id, entry);
            }
        }
    }

    let document_entry =
        document_entry.ok_or_else(|| validation("project archive is missing document.json"))?;
    let mut budget = ArchiveBudget::new();
    budget.reserve_document_json(document_entry.size)?;
    let metadata_bytes = read_entry(&mut archive, &document_entry, MAX_DOCUMENT_JSON_BYTES)?;
    let metadata_json = str::from_utf8(&metadata_bytes)
        .map_err(|error| validation(format!("document.json is not UTF-8: {error}")))?;
    let document_snapshot = PtzDocument::from_json(metadata_json)
        .map_err(|error| validation(format!("invalid document.json: {error}")))?;
    validate_document_envelope(metadata_json)?;
    let content = PtzDocumentContent::from_ptz_document_json(metadata_json)
        .map_err(|error| validation(format!("invalid document.json: {error}")))?;

    let rasters = validate_content(&content, layer_entries, &mut budget)?;

    let document_id = content.id.clone();
    let mut registry = PixelStoreRegistry::new();
    registry.open_document(&document_id);
    for raster in &rasters {
        let pixels = if raster.width == 0 {
            Vec::new()
        } else {
            decode_raster(
                &mut archive,
                raster.entry.as_ref().ok_or_else(|| {
                    ServiceError::Internal("validated raster has no PNG entry".into())
                })?,
                raster,
            )?
        };
        registry
            .add_layer(
                &document_id,
                &raster.id,
                raster.width,
                raster.height,
                pixels,
            )
            .map_err(|error| validation(format!("cannot seed raster pixels: {error}")))?;
    }
    registry
        .seed_document(&document_id, content)
        .map_err(|error| validation(format!("cannot seed document metadata: {error}")))?;

    let content = registry
        .document_content(&document_id)
        .ok_or_else(|| ServiceError::Internal("seeded document content is missing".into()))?;
    let composite = photrez_core::render::compositor::composite_layers_with_font_data(
        content,
        &registry,
        Some(crate::INTER),
    )
    .map_err(|error| {
        gap(&format!(
            "the supported layer stack could not be rendered: {error}"
        ))
    })?;

    Ok(LoadedDocument {
        registry,
        document_id,
        display_width: composite.width,
        display_height: composite.height,
        display_rgba: composite.pixels,
        document_snapshot,
    })
}

fn validate_document_envelope(metadata_json: &str) -> Result<(), ServiceError> {
    let envelope: serde_json::Value = serde_json::from_str(metadata_json)
        .map_err(|error| validation(format!("invalid document.json: {error}")))?;
    if envelope.get("format").and_then(serde_json::Value::as_str) != Some(PTZ_FORMAT) {
        return Err(validation("unsupported document format"));
    }
    let version = envelope
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| validation("invalid document version"))?;
    if !(1..=MAX_SUPPORTED_PTZ_VERSION).contains(&version) {
        return Err(validation("unsupported document version"));
    }
    Ok(())
}

fn parse_entry_name(name: &str) -> Result<EntryName, ServiceError> {
    if name == "document.json" {
        return Ok(EntryName::Document);
    }
    let Some(file_name) = name.strip_prefix("layers/") else {
        return Err(validation("unexpected or unsafe project archive entry"));
    };
    let Some(layer_id) = file_name.strip_suffix(".png") else {
        return Err(validation("unexpected or unsafe project archive entry"));
    };
    if layer_id.contains('/') || layer_id.contains('\\') {
        return Err(validation("unexpected or unsafe project archive entry"));
    }
    ptz_limits::validate_layer_id(layer_id)?;
    Ok(EntryName::Layer(layer_id.to_owned()))
}

fn validate_content(
    content: &PtzDocumentContent,
    mut layer_entries: HashMap<String, ArchiveEntry>,
    budget: &mut ArchiveBudget,
) -> Result<Vec<RasterMetadata>, ServiceError> {
    if content.id.is_empty() {
        return Err(validation("document ID must not be empty"));
    }
    if content.layers.len() > MAX_LAYER_COUNT {
        return Err(resource_limit("project layer limit exceeded"));
    }
    let document_width = integer_dimension(content.width, MAX_LAYER_DIMENSION, "document width")?;
    let document_height =
        integer_dimension(content.height, MAX_LAYER_DIMENSION, "document height")?;
    if (document_width == 0) != (document_height == 0) {
        return Err(validation(
            "document dimensions must both be zero or nonzero",
        ));
    }

    let mut ids = HashSet::with_capacity(content.layers.len());
    let mut rasters = Vec::new();
    for layer in &content.layers {
        ptz_limits::validate_layer_id(&layer.id)?;
        if !ids.insert(layer.id.as_str()) {
            return Err(validation("document contains duplicate layer IDs"));
        }
        match &layer.layer_type {
            LayerType::Raster => {
                let width = integer_dimension(layer.width, MAX_LAYER_DIMENSION, "raster width")?;
                let height = integer_dimension(layer.height, MAX_LAYER_DIMENSION, "raster height")?;
                if (width == 0) != (height == 0) {
                    return Err(validation("raster dimensions must both be zero or nonzero"));
                }
                let entry = layer_entries.remove(&layer.id);
                if width == 0 {
                    if entry.is_some() {
                        return Err(validation("empty raster must not have a PNG entry"));
                    }
                    rasters.push(RasterMetadata {
                        id: layer.id.clone(),
                        width,
                        height,
                        entry: None,
                    });
                } else {
                    let entry = entry.ok_or_else(|| validation("raster PNG entry is missing"))?;
                    budget.reserve_layer(
                        &layer.id,
                        u64::from(width),
                        u64::from(height),
                        entry.size,
                    )?;
                    rasters.push(RasterMetadata {
                        id: layer.id.clone(),
                        width,
                        height,
                        entry: Some(entry),
                    });
                }
            }
            LayerType::Shape => {
                if layer.shape_params.is_none() {
                    return Err(validation("shape layer is missing its parameters"));
                }
            }
            LayerType::Text => {
                if layer.text_data.is_none() {
                    return Err(validation("text layer is missing its text data"));
                }
            }
            LayerType::Adjustment | LayerType::Group => {
                return Err(gap(
                    "the native display path supports raster, shape, and text layers",
                ));
            }
        }
    }
    if !layer_entries.is_empty() {
        return Err(validation(
            "project raster entries do not match raster metadata",
        ));
    }
    Ok(rasters)
}

pub(super) fn integer_dimension(
    value: f64,
    maximum: u64,
    label: &str,
) -> Result<u32, ServiceError> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > maximum as f64 {
        return Err(validation(format!("invalid {label}")));
    }
    Ok(value as u32)
}

fn decode_raster<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry: &ArchiveEntry,
    raster: &RasterMetadata,
) -> Result<Vec<u8>, ServiceError> {
    let png_bytes = read_entry(archive, entry, MAX_ARCHIVE_ENTRY_BYTES)?;
    if !png_bytes.starts_with(PNG_SIGNATURE) {
        return Err(validation("raster entry does not have a PNG signature"));
    }
    let dimensions = png_dimensions(&png_bytes)
        .map_err(|error| validation(format!("invalid raster PNG header: {error}")))?;
    if (dimensions.width(), dimensions.height()) != (raster.width, raster.height) {
        return Err(validation(
            "raster PNG dimensions do not match document metadata",
        ));
    }
    let decoded_size =
        ptz_limits::decoded_rgba_bytes(u64::from(raster.width), u64::from(raster.height))?;
    let capacity = usize::try_from(decoded_size)
        .map_err(|_| resource_limit("decoded raster size does not fit in memory"))?;
    let mut pixels = vec![0; capacity];
    let decoded = decode_png_into(&png_bytes, &mut pixels)
        .map_err(|error| validation(format!("invalid raster PNG data: {error}")))?;
    if (decoded.width(), decoded.height()) != (raster.width, raster.height) {
        return Err(validation(
            "decoded raster dimensions do not match document metadata",
        ));
    }
    Ok(pixels)
}

fn read_entry<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry: &ArchiveEntry,
    maximum: u64,
) -> Result<Vec<u8>, ServiceError> {
    if entry.size > maximum {
        return Err(resource_limit(
            "project archive entry exceeds its byte limit",
        ));
    }
    let mut file = archive.by_index(entry.index).map_err(invalid_archive)?;
    let declared_size = file.size();
    if declared_size != entry.size {
        return Err(validation("project archive entry metadata changed"));
    }
    let limit = declared_size.saturating_add(1);
    let mut bytes = Vec::new();
    file.by_ref()
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(invalid_archive)?;
    let actual_size = u64::try_from(bytes.len()).map_err(|_| {
        resource_limit("project archive entry size does not fit in file-size range")
    })?;
    if actual_size > maximum {
        return Err(resource_limit(
            "project archive entry exceeds its byte limit",
        ));
    }
    if actual_size != declared_size {
        return Err(validation(
            "project archive entry size does not match its contents",
        ));
    }
    Ok(bytes)
}

fn preflight_central_directory_entry_count(bytes: &[u8]) -> Result<usize, ServiceError> {
    const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
    const ZIP64_LOCATOR_SIGNATURE: &[u8; 4] = b"PK\x06\x07";
    const EOCD_SIZE: usize = 22;
    // zip 9.0.4 scans Zip32CentralDirectoryEnd + 4 + 65,536. Its x64 layout is
    // 32 bytes (65,572 total); 128 KiB exceeds that and narrower 32-bit layouts.
    const EOCD_SEARCH_WINDOW_BYTES: usize = 128 * 1024;

    let search_start = bytes.len().saturating_sub(EOCD_SEARCH_WINDOW_BYTES);
    let search_end = bytes
        .len()
        .checked_sub(4)
        .ok_or_else(|| validation("project archive has no end record"))?;
    let mut end_offset = None;
    for offset in search_start..=search_end {
        if bytes.get(offset..offset + 4) == Some(EOCD_SIGNATURE.as_slice())
            && end_offset.replace(offset).is_some()
        {
            return Err(validation("project archive has ambiguous end records"));
        }
    }
    let end_offset = end_offset.ok_or_else(|| validation("project archive has no end record"))?;
    // ZIP readers may retry earlier EOCDs after directory failure. Extra signatures,
    // including coincidental bytes in the comment, are rejected conservatively.
    let record_end = end_offset
        .checked_add(EOCD_SIZE)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| validation("project archive has no valid end record"))?;
    let comment_size = read_u16(bytes, end_offset + 20)?;
    if record_end.checked_add(usize::from(comment_size)) != Some(bytes.len()) {
        return Err(validation("project archive has no valid end record"));
    }
    let disk = read_u16(bytes, end_offset + 4)?;
    let central_disk = read_u16(bytes, end_offset + 6)?;
    let disk_entries = read_u16(bytes, end_offset + 8)?;
    let total_entries = read_u16(bytes, end_offset + 10)?;
    if disk != 0 || central_disk != 0 {
        return Err(validation("multi-disk project archives are not supported"));
    }
    if disk_entries == u16::MAX || total_entries == u16::MAX {
        return Err(validation("ZIP64 project archives are not supported"));
    }
    if disk_entries != total_entries {
        return Err(validation("multi-disk project archives are not supported"));
    }
    let locator_offset = end_offset.checked_sub(20);
    if locator_offset.is_some_and(|offset| {
        bytes.get(offset..offset + 4) == Some(ZIP64_LOCATOR_SIGNATURE.as_slice())
    }) {
        return Err(validation("ZIP64 project archives are not supported"));
    }

    let entry_count = usize::from(total_entries);
    if entry_count > MAX_STRUCTURAL_ENTRIES {
        return Err(resource_limit("project archive entry limit exceeded"));
    }
    Ok(entry_count)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ServiceError> {
    let bytes: [u8; 2] = bytes
        .get(offset..offset.saturating_add(2))
        .and_then(|slice| slice.try_into().ok())
        .ok_or_else(|| validation("project archive end record is truncated"))?;
    Ok(u16::from_le_bytes(bytes))
}

fn invalid_archive(error: impl std::fmt::Display) -> ServiceError {
    validation(format!("invalid project archive: {error}"))
}

pub(super) fn validation(message: impl Into<String>) -> ServiceError {
    ServiceError::Validation(message.into())
}

pub(super) fn resource_limit(message: impl Into<String>) -> ServiceError {
    ServiceError::ResourceLimit(message.into())
}

pub(super) fn gap(message: &str) -> ServiceError {
    validation(format!("GAP: {message}"))
}
