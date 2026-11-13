// SPDX-License-Identifier: AGPL-3.0-or-later
//! Destructive crop (`delete_cropped_pixels`) end to end: the metadata arm and
//! the pixel bake land in ONE history entry, so a single undo restores both.

use crate::pixel::store::PixelStoreRegistry;
use crate::protocol::{Command, CommandEnvelope, CONTRACT_VERSION};

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
            layer_type: None,
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
    for chunk in pixels.as_chunks_mut::<4>().0.iter_mut() {
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

fn alpha_at(reg: &PixelStoreRegistry, x: u32, y: u32) -> u8 {
    let layer = reg
        .docs
        .get("d")
        .and_then(|s| s.layers.get("bg"))
        .expect("layer pixels");
    layer.pixels[((y * layer.width + x) * 4) as usize + 3]
}

#[test]
fn destructive_crop_discards_pixels_outside_the_new_document() {
    let mut reg = seeded_store_with(255);
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(crop(true)))
        .expect("crop applies");

    assert_eq!(
        reg.docs.get("d").unwrap().history.doc_size,
        Some((4.0, 4.0)),
        "document shrank to the crop rect"
    );
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
    assert_eq!(alpha_at(&reg, 4, 0), 0, "first column outside is discarded");
    assert_eq!(alpha_at(&reg, 0, 4), 0, "first row outside is discarded");
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
    assert_eq!(alpha_at(&reg, 5, 5), 0, "redo discards them again");
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
