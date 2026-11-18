//! Destructive crop (`delete_cropped_pixels`) end to end: the metadata arm and
//! the pixel bake land in ONE history entry, so a single undo restores both.

use crate::pixel::store::PixelStoreRegistry;
use crate::protocol::{CONTRACT_VERSION, Command, CommandEnvelope};

fn envelope(command: Command) -> CommandEnvelope {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command,
    }
}

fn crop_with(delete_cropped_pixels: bool, fill_background: Option<[u8; 4]>) -> Command {
    Command::ApplyCrop {
        x: 0.0,
        y: 0.0,
        width: 4.0,
        height: 4.0,
        rotation: None,
        target_width: None,
        target_height: None,
        delete_cropped_pixels,
        fill_background,
    }
}

fn crop(delete_cropped_pixels: bool) -> Command {
    crop_with(delete_cropped_pixels, None)
}

fn seeded_store_with(alpha: u8) -> PixelStoreRegistry {
    let mut reg = PixelStoreRegistry::new();
    reg.open_document("d");
    let store = reg.docs.get_mut("d").expect("doc open");
    store
        .history
        .apply(envelope(Command::AddLayer {
            id: "bg".to_string(),
            name: "bg".to_string(),
            width: 8.0,
            height: 8.0,
            index: 0,
            x: Some(0.0),
            y: Some(0.0),
            layer_type: Some(crate::document::content::LayerType::Raster),
            shape_params: None,
            text_data: None,
        }))
        .expect("add layer");
    store
        .history
        .apply(envelope(Command::SetBackgroundFlag {
            id: "bg".to_string(),
        }))
        .expect("mark background");
    let mut pixels = vec![0u8; 8 * 8 * 4];
    let (chunks, _) = pixels.as_chunks_mut::<4>();
    for chunk in chunks {
        chunk[0] = 9;
        chunk[1] = 9;
        chunk[2] = 9;
        chunk[3] = alpha;
    }
    store.init_layer("bg", 8, 8, pixels);
    // An opened document has a canvas size; the crop entry snapshots it so
    // undo can restore it.
    store.history.doc_size = Some((8.0, 8.0));
    reg
}

/// A background whose red channel is the pixel column, so a shifted bake is
/// visible in the bytes.
fn seeded_store_gradient() -> PixelStoreRegistry {
    let mut reg = seeded_store_with(255);
    let layer = reg
        .docs
        .get_mut("d")
        .and_then(|s| s.layers.get_mut("bg"))
        .expect("layer pixels");
    for x in 0..8u32 {
        for y in 0..8u32 {
            let at = ((y * 8 + x) * 4) as usize;
            layer.pixels[at] = (x * 10) as u8;
        }
    }
    reg
}

fn pixel_at(reg: &PixelStoreRegistry, x: u32, y: u32) -> [u8; 4] {
    let layer = reg
        .docs
        .get("d")
        .and_then(|s| s.layers.get("bg"))
        .expect("layer pixels");
    let at = ((y * layer.width + x) * 4) as usize;
    [
        layer.pixels[at],
        layer.pixels[at + 1],
        layer.pixels[at + 2],
        layer.pixels[at + 3],
    ]
}

#[test]
fn destructive_crop_bakes_rasters_to_the_new_document_size() {
    let mut reg = seeded_store_with(255);
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(crop(true)))
        .expect("crop applies");

    let store = reg.docs.get("d").unwrap();
    assert_eq!(
        store.history.doc_size,
        Some((4.0, 4.0)),
        "document shrank to the crop rect"
    );
    let pixels = store.layers.get("bg").unwrap();
    assert_eq!((pixels.width, pixels.height), (4, 4));
    assert_eq!(pixels.pixels.len(), 4 * 4 * 4);
    assert_eq!(
        pixel_at(&reg, 0, 0),
        [9, 9, 9, 255],
        "pixel inside survives"
    );
    assert_eq!(
        pixel_at(&reg, 3, 3),
        [9, 9, 9, 255],
        "last inside pixel survives"
    );
    let layer = store.history.layers.get(0).unwrap();
    assert_eq!(
        (layer.x, layer.y, layer.scale_x, layer.scale_y),
        (0.0, 0.0, 1.0, 1.0)
    );
    assert_eq!(layer.rotation, 0.0);
}

#[test]
fn destructive_crop_history_trimming_keeps_undo_and_redo_pixels() {
    let mut reg = seeded_store_with(255);
    reg.docs.get_mut("d").unwrap().history.max_entries = 2;
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(crop(true)))
        .expect("crop applies");

    assert_eq!(reg.docs.get("d").unwrap().history.entries.len(), 2);
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::Undo))
        .expect("undo the crop");
    let store = reg.docs.get("d").unwrap();
    assert_eq!(store.history.doc_size, Some((8.0, 8.0)));
    let pixels = store.layers.get("bg").unwrap();
    assert_eq!((pixels.width, pixels.height), (8, 8));
    assert_eq!(pixel_at(&reg, 5, 5), [9, 9, 9, 255]);

    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::Redo))
        .expect("redo the crop");
    let pixels = reg.docs.get("d").unwrap().layers.get("bg").unwrap();
    assert_eq!((pixels.width, pixels.height), (4, 4));
    assert_eq!(pixel_at(&reg, 3, 3), [9, 9, 9, 255]);
}

#[test]
fn destructive_crop_rejects_corrupt_raster_pixels_before_history_changes() {
    let mut reg = seeded_store_with(255);
    let store = reg.docs.get_mut("d").expect("doc open");
    store
        .get_layer_mut("bg")
        .expect("layer pixels")
        .pixels
        .pop();
    let version = store.history.version();

    let error = store
        .apply_command(envelope(crop(true)))
        .expect_err("invalid raster bytes must reject the crop");

    assert_eq!(error.code, "E_INVALID");
    assert_eq!(store.history.doc_size, Some((8.0, 8.0)));
    assert_eq!(store.history.version(), version);
    assert_eq!(store.history.layers.get(0).unwrap().width, Some(8.0));

    let fill_error = store
        .apply_command(envelope(crop_with(false, Some([0, 0, 255, 255]))))
        .expect_err("fill must reject corrupt Background pixels");
    assert_eq!(fill_error.code, "E_INVALID");
    assert_eq!(store.history.doc_size, Some((8.0, 8.0)));
    assert_eq!(store.history.version(), version);
}

#[test]
fn destructive_crop_rejects_missing_raster_pixels_before_history_changes() {
    let mut reg = seeded_store_with(255);
    let store = reg.docs.get_mut("d").expect("doc open");
    store.remove_layer("bg");
    let version = store.history.version();

    let error = store
        .apply_command(envelope(crop(true)))
        .expect_err("missing raster bytes must reject the crop");

    assert_eq!(error.code, "E_INVALID");
    assert_eq!(store.history.doc_size, Some((8.0, 8.0)));
    assert_eq!(store.history.version(), version);
    assert_eq!(store.history.layers.get(0).unwrap().width, Some(8.0));

    let fill_error = store
        .apply_command(envelope(crop_with(false, Some([0, 0, 255, 255]))))
        .expect_err("fill must reject missing Background pixels");
    assert_eq!(fill_error.code, "E_INVALID");
    assert_eq!(store.history.doc_size, Some((8.0, 8.0)));
    assert_eq!(store.history.version(), version);
}

#[test]
fn crop_preflight_preserves_validation_and_noop_precedence() {
    let mut reg = seeded_store_with(255);
    let store = reg.docs.get_mut("d").expect("doc open");
    store
        .get_layer_mut("bg")
        .expect("layer pixels")
        .pixels
        .pop();
    let version = store.history.version();
    let entries = store.history.entries.len();

    let error = store
        .apply_command(envelope(Command::ApplyCrop {
            x: 0.0,
            y: 0.0,
            width: 4.0,
            height: 4.0,
            rotation: None,
            target_width: Some(4.0),
            target_height: None,
            delete_cropped_pixels: true,
            fill_background: None,
        }))
        .expect_err("the invalid target pair must be rejected before pixel staging");

    assert_eq!(error.code, "E_INVALID");
    assert_eq!(
        error.message,
        "applyCrop target size requires both targetWidth and targetHeight"
    );
    assert_eq!(store.history.version(), version);
    assert_eq!(store.history.entries.len(), entries);
    assert_eq!(store.history.doc_size, Some((8.0, 8.0)));

    let noop = store
        .apply_command(envelope(Command::ApplyCrop {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 4.0,
            rotation: None,
            target_width: None,
            target_height: None,
            delete_cropped_pixels: true,
            fill_background: None,
        }))
        .expect("a non-positive crop remains a no-op before pixel staging");

    assert!(noop.delta.changes.is_empty());
    assert_eq!(store.history.entries.len(), entries);
    assert_eq!(store.history.doc_size, Some((8.0, 8.0)));
}

#[test]
fn fill_rejects_a_non_raster_background_before_history_changes() {
    let mut reg = seeded_store_with(255);
    let store = reg.docs.get_mut("d").expect("doc open");
    let mut background = store.history.layers.get(0).unwrap().clone();
    background.layer_type = Some(crate::document::content::LayerType::Shape);
    store.history.layers = store.history.layers.replaced(0, background);
    let version = store.history.version();
    let original_pixels = store.layers.get("bg").unwrap().pixels.clone();

    let error = store
        .apply_command(envelope(crop_with(false, Some([0, 0, 255, 255]))))
        .expect_err("fill needs a raster Background");

    assert_eq!(error.code, "E_INVALID");
    assert_eq!(store.history.doc_size, Some((8.0, 8.0)));
    assert_eq!(store.history.version(), version);
    assert_eq!(
        store.history.layers.get(0).unwrap().layer_type,
        Some(crate::document::content::LayerType::Shape)
    );
    assert_eq!(store.layers.get("bg").unwrap().pixels, original_pixels);
}

#[test]
fn destructive_fill_materializes_adjustment_once_and_undo_restores_it() {
    let mut reg = seeded_store_with(255);
    let store = reg.docs.get_mut("d").expect("doc open");
    let mut background = store.history.layers.get(0).unwrap().clone();
    background.basic_adjustment = Some(crate::document::layer_params::BasicAdjustment {
        brightness: 100.0,
        contrast: 0.0,
        saturation: 0.0,
    });
    background.has_adjustments = Some(true);
    store.history.layers = store.history.layers.replaced(0, background);

    store
        .apply_command(envelope(crop_with(true, Some([0, 0, 255, 255]))))
        .expect("crop and fill apply");
    assert_eq!(pixel_at(&reg, 0, 0), [132, 132, 132, 255]);
    assert_eq!(
        reg.docs
            .get("d")
            .unwrap()
            .history
            .layers
            .get(0)
            .unwrap()
            .basic_adjustment,
        None
    );

    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::Undo))
        .expect("undo crop and fill");
    assert_eq!(pixel_at(&reg, 7, 7), [9, 9, 9, 255]);
    assert_eq!(
        reg.docs
            .get("d")
            .unwrap()
            .history
            .layers
            .get(0)
            .unwrap()
            .basic_adjustment
            .as_ref()
            .unwrap()
            .brightness,
        100.0
    );
}

#[test]
fn destructive_fill_keeps_background_opacity_and_blend_metadata() {
    let mut reg = seeded_store_with(255);
    let store = reg.docs.get_mut("d").expect("doc open");
    let mut background = store.history.layers.get(0).unwrap().clone();
    background.opacity = 0.5;
    background.blend_mode = Some(crate::document::content::BlendMode::Multiply);
    store.history.layers = store.history.layers.replaced(0, background);

    store
        .apply_command(envelope(crop_with(true, Some([0, 0, 255, 255]))))
        .expect("crop and fill apply");

    let store = reg.docs.get("d").expect("doc open");
    assert_eq!(pixel_at(&reg, 0, 0), [0, 0, 132, 255]);
    let background = store.history.layers.get(0).unwrap();
    assert_eq!(background.opacity, 0.5);
    assert_eq!(
        background.blend_mode,
        Some(crate::document::content::BlendMode::Multiply)
    );
}

#[test]
fn destructive_crop_keeps_ordinary_raster_metadata() {
    let mut reg = seeded_store_with(255);
    let store = reg.docs.get_mut("d").expect("doc open");
    let mut layer = store.history.layers.get(0).unwrap().clone();
    layer.is_background = Some(false);
    layer.lock_position = Some(false);
    layer.lock_rotation = Some(false);
    layer.visible = false;
    layer.opacity = 0.75;
    layer.blend_mode = Some(crate::document::content::BlendMode::Multiply);
    store.history.layers = store.history.layers.replaced(0, layer);

    store
        .apply_command(envelope(crop(true)))
        .expect("destructive crop applies");

    let store = reg.docs.get("d").expect("doc open");
    let layer = store.history.layers.get(0).unwrap();
    assert!(!layer.visible);
    assert_eq!(layer.opacity, 0.75);
    assert_eq!(
        layer.blend_mode,
        Some(crate::document::content::BlendMode::Multiply)
    );
    assert_eq!(layer.is_background, Some(false));
    let pixels = store.layers.get("bg").unwrap();
    assert_eq!((pixels.width, pixels.height), (4, 4));
}

#[test]
fn one_undo_restores_the_metadata_and_the_discarded_pixels() {
    let mut reg = seeded_store_with(255);
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(crop(true)))
        .expect("crop applies");
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::Undo))
        .expect("undo the crop");

    assert_eq!(
        reg.docs.get("d").unwrap().history.doc_size,
        Some((8.0, 8.0)),
        "undo restores the document size"
    );
    assert_eq!(
        pixel_at(&reg, 5, 5),
        [9, 9, 9, 255],
        "undo restores the pixels the crop discarded"
    );

    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::Redo))
        .expect("redo the crop");
    assert_eq!(
        reg.docs.get("d").unwrap().history.doc_size,
        Some((4.0, 4.0)),
        "redo re-applies the crop"
    );
    let layer = reg.docs.get("d").unwrap().layers.get("bg").unwrap();
    assert_eq!((layer.width, layer.height), (4, 4));
    assert_eq!(pixel_at(&reg, 3, 3), [9, 9, 9, 255]);
}

#[test]
fn a_non_destructive_crop_keeps_every_pixel() {
    let mut reg = seeded_store_with(255);
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(crop(false)))
        .expect("crop applies");

    assert_eq!(
        reg.docs.get("d").unwrap().history.doc_size,
        Some((4.0, 4.0)),
        "the document still shrinks"
    );
    assert_eq!(
        pixel_at(&reg, 5, 5),
        [9, 9, 9, 255],
        "without the flag the pixels stay (non-destructive branch)"
    );
}

#[test]
fn fill_background_is_invisible_under_an_opaque_background() {
    let mut reg = seeded_store_with(255);
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(crop_with(false, Some([255, 0, 0, 255]))))
        .expect("crop with fill");

    assert_eq!(
        pixel_at(&reg, 0, 0),
        [9, 9, 9, 255],
        "opaque Background content stays on top of the fill"
    );
    let layer = reg.docs.get("d").unwrap().layers.get("bg").unwrap();
    assert_eq!((layer.width, layer.height), (4, 4), "baked to the new size");
}

#[test]
fn fill_background_shows_through_a_transparent_background() {
    let mut reg = seeded_store_with(0);
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(crop_with(false, Some([255, 0, 0, 255]))))
        .expect("crop with fill");

    assert_eq!(
        pixel_at(&reg, 0, 0),
        [255, 0, 0, 255],
        "the fill is what the cropped canvas shows"
    );
}

#[test]
fn fill_bake_preserves_background_visibility() {
    let mut reg = seeded_store_with(255);
    let store = reg.docs.get_mut("d").expect("doc open");
    let mut background = store.history.layers.get(0).unwrap().clone();
    background.visible = false;
    store.history.layers = store.history.layers.replaced(0, background);

    store
        .apply_command(envelope(crop_with(false, Some([255, 0, 0, 255]))))
        .expect("crop with fill");

    assert_eq!(pixel_at(&reg, 0, 0), [255, 0, 0, 255]);
    assert!(
        !reg.docs
            .get("d")
            .unwrap()
            .history
            .layers
            .get(0)
            .unwrap()
            .visible
    );
}

#[test]
fn undo_restores_the_background_size_and_bytes_after_a_fill() {
    let mut reg = seeded_store_with(0);
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(crop_with(false, Some([255, 0, 0, 255]))))
        .expect("crop with fill");
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::Undo))
        .expect("undo the fill crop");

    let layer = reg.docs.get("d").unwrap().layers.get("bg").unwrap();
    assert_eq!(
        (layer.width, layer.height),
        (8, 8),
        "undo restores the original bitmap size"
    );
    assert_eq!(
        pixel_at(&reg, 5, 5),
        [9, 9, 9, 0],
        "and the original bytes, not the baked fill"
    );
}

/// A crop that does not start at the page origin keeps the content where it
/// was: the baked background shows the columns the crop took.
#[test]
fn the_fill_bake_keeps_the_content_at_the_cropped_columns() {
    let mut reg = seeded_store_gradient();
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::ApplyCrop {
            x: 2.0,
            y: 0.0,
            width: 4.0,
            height: 4.0,
            rotation: None,
            target_width: None,
            target_height: None,
            delete_cropped_pixels: false,
            fill_background: Some([0, 0, 255, 255]),
        }))
        .expect("crop with fill");

    // The crop took the columns 2..6 of the page, so the baked background
    // starts at the red value of column 2.
    assert_eq!(
        pixel_at(&reg, 0, 0),
        [20, 9, 9, 255],
        "the baked background starts at the cropped column"
    );
    assert_eq!(
        pixel_at(&reg, 3, 0),
        [50, 9, 9, 255],
        "and ends at the last cropped column"
    );
}

#[test]
fn fill_bake_keeps_a_locked_background_in_place() {
    let mut reg = seeded_store_gradient();
    let store = reg.docs.get_mut("d").expect("doc open");
    let mut background = store.history.layers.get(0).unwrap().clone();
    background.locked = Some(true);
    store.history.layers = store.history.layers.replaced(0, background);

    store
        .apply_command(envelope(Command::ApplyCrop {
            x: 2.0,
            y: 0.0,
            width: 4.0,
            height: 4.0,
            rotation: None,
            target_width: None,
            target_height: None,
            delete_cropped_pixels: false,
            fill_background: Some([0, 0, 255, 255]),
        }))
        .expect("crop with fill");

    assert_eq!(pixel_at(&reg, 0, 0), [0, 9, 9, 255]);
    assert_eq!(pixel_at(&reg, 3, 0), [30, 9, 9, 255]);
    let background = reg.docs.get("d").unwrap().history.layers.get(0).unwrap();
    assert_eq!(background.locked, Some(true));
    assert_eq!((background.x, background.y), (0.0, 0.0));
}

/// A crop that reaches past the page edge fills the new area with the fill
/// colour instead of smearing the edge of the background.
#[test]
fn the_fill_bake_fills_the_area_past_the_page_edge() {
    let mut reg = seeded_store_gradient();
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::ApplyCrop {
            x: -2.0,
            y: 0.0,
            width: 12.0,
            height: 4.0,
            rotation: None,
            target_width: None,
            target_height: None,
            delete_cropped_pixels: false,
            fill_background: Some([0, 0, 255, 255]),
        }))
        .expect("crop with fill");

    let layer = reg.docs.get("d").unwrap().layers.get("bg").unwrap();
    assert_eq!((layer.width, layer.height), (12, 4), "the document grew");
    // The crop starts two columns before the page, so the first two columns
    // of the new document hold the fill colour and the page content follows.
    assert_eq!(
        pixel_at(&reg, 0, 0),
        [0, 0, 255, 255],
        "the area before the page shows the fill colour"
    );
    assert_eq!(
        pixel_at(&reg, 2, 0),
        [0, 9, 9, 255],
        "the page content keeps its place"
    );
    assert_eq!(
        pixel_at(&reg, 9, 0),
        [70, 9, 9, 255],
        "the last page column is at the end of the content"
    );
    assert_eq!(
        pixel_at(&reg, 10, 0),
        [0, 0, 255, 255],
        "the area past the page edge shows the fill colour"
    );
}

#[test]
fn fill_bake_fills_the_corners_outside_a_rotated_background() {
    let mut reg = seeded_store_with(255);
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::ApplyCrop {
            x: 0.0,
            y: 0.0,
            width: 8.0,
            height: 8.0,
            rotation: Some(45.0),
            target_width: None,
            target_height: None,
            delete_cropped_pixels: false,
            fill_background: Some([0, 0, 255, 255]),
        }))
        .expect("crop with fill");

    assert_eq!(pixel_at(&reg, 0, 0), [0, 0, 255, 255]);
    assert_eq!(pixel_at(&reg, 3, 3), [9, 9, 9, 255]);
}

#[test]
fn fill_background_creates_a_background_when_missing_and_round_trips_history() {
    let mut reg = PixelStoreRegistry::new();
    reg.open_document("d");
    reg.docs.get_mut("d").expect("doc open").history.doc_size = Some((8.0, 8.0));

    reg.docs
        .get_mut("d")
        .expect("doc open")
        .apply_command(envelope(crop_with(false, Some([255, 0, 0, 255]))))
        .expect("crop with fill");

    let store = reg.docs.get("d").expect("doc open");
    assert_eq!(store.history.entries.len(), 1, "crop is one history step");
    assert_eq!(store.history.doc_size(), Some((4.0, 4.0)));
    let background = store.history.layers.get(0).expect("created Background");
    assert_eq!(background.name, "Background");
    assert_eq!(background.is_background, Some(true));
    let background_id = background.id.clone();
    let pixels = store
        .layers
        .get(&background_id)
        .expect("created Background pixels");
    assert_eq!((pixels.width, pixels.height), (4, 4));
    assert!(
        pixels
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [255, 0, 0, 255])
    );

    reg.docs
        .get_mut("d")
        .expect("doc open")
        .apply_command(envelope(Command::Undo))
        .expect("undo crop");
    let store = reg.docs.get("d").expect("doc open");
    assert_eq!(store.history.doc_size(), Some((8.0, 8.0)));
    assert!(store.history.layers.get(0).is_none());

    reg.docs
        .get_mut("d")
        .expect("doc open")
        .apply_command(envelope(Command::Redo))
        .expect("redo crop");
    let store = reg.docs.get("d").expect("doc open");
    let background = store.history.layers.get(0).expect("restored Background");
    assert_eq!(background.id, background_id);
    let pixels = store
        .layers
        .get(&background_id)
        .expect("restored Background pixels");
    assert_eq!((pixels.width, pixels.height), (4, 4));
    assert!(
        pixels
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [255, 0, 0, 255])
    );
}

#[test]
fn fill_background_with_normal_layers_keeps_pixel_history_after_creation() {
    let mut reg = PixelStoreRegistry::new();
    reg.open_document("d");
    let store = reg.docs.get_mut("d").expect("doc open");
    let normal_id = "crop-fill-background-2";
    let normal = crate::protocol::LayerMeta {
        id: normal_id.to_string(),
        name: "Normal".to_string(),
        resource_id: 1,
        width: Some(8.0),
        height: Some(8.0),
        layer_type: Some(crate::document::content::LayerType::Raster),
        ..Default::default()
    };
    store.history.seed_layers(vec![normal], 0);
    let normal_pixels = vec![73; 8 * 8 * 4];
    store.init_layer(normal_id, 8, 8, normal_pixels.clone());
    store.history.doc_size = Some((8.0, 8.0));

    reg.docs
        .get_mut("d")
        .expect("doc open")
        .apply_command(envelope(crop_with(false, Some([255, 0, 0, 255]))))
        .expect("crop with fill");

    let store = reg.docs.get("d").expect("doc open");
    assert_eq!(store.history.entries.len(), 1, "crop is one history step");
    assert_eq!(
        store.history.layers.get(0).expect("normal layer").id,
        normal_id
    );
    let background = store.history.layers.get(1).expect("created Background");
    assert_eq!(background.is_background, Some(true));
    let background_id = background.id.clone();
    assert_eq!(background_id, "crop-fill-background-2-1");
    let pixels = store
        .layers
        .get(&background_id)
        .expect("created Background pixels");
    assert!(
        pixels
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [255, 0, 0, 255])
    );
    assert_eq!(
        store.layers.get(normal_id).expect("normal pixels").pixels,
        normal_pixels
    );

    reg.write_region("d", &background_id, 1, 1, 1, 1, vec![0, 0, 255, 255])
        .expect("edit the created Background");
    reg.docs
        .get_mut("d")
        .expect("doc open")
        .apply_command(envelope(Command::Undo))
        .expect("undo Background edit");
    assert_eq!(
        &reg.docs.get("d").expect("doc open").layers[&background_id].pixels[20..24],
        &[255, 0, 0, 255]
    );

    reg.docs
        .get_mut("d")
        .expect("doc open")
        .apply_command(envelope(Command::Undo))
        .expect("undo crop");
    let store = reg.docs.get("d").expect("doc open");
    assert_eq!(store.history.doc_size(), Some((8.0, 8.0)));
    assert!(store.history.layers.get(1).is_none());
    assert_eq!(
        store.history.layers.get(0).expect("normal layer").id,
        normal_id
    );

    reg.docs
        .get_mut("d")
        .expect("doc open")
        .apply_command(envelope(Command::Redo))
        .expect("redo crop");
    reg.docs
        .get_mut("d")
        .expect("doc open")
        .apply_command(envelope(Command::Redo))
        .expect("redo Background edit");
    let pixels = &reg.docs.get("d").expect("doc open").layers[&background_id].pixels;
    assert_eq!(&pixels[20..24], &[0, 0, 255, 255]);
}
