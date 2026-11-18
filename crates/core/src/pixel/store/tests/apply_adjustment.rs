// SPDX-License-Identifier: AGPL-3.0-or-later
//! `applyBasicAdjustment` end to end: the metadata arm and the baked pixel
//! land in ONE history entry, so a single undo restores both.

use crate::document::content::BasicAdjustment;
use crate::pixel::store::PixelStoreRegistry;
use crate::protocol::{CONTRACT_VERSION, Command, CommandEnvelope};
use crate::wire::command::RasterizePixels;

fn envelope(command: Command) -> CommandEnvelope {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command,
    }
}

fn seeded_store_with(pixels: &[u8]) -> PixelStoreRegistry {
    let mut reg = PixelStoreRegistry::new();
    reg.open_document("d");
    let store = reg.docs.get_mut("d").expect("doc open");
    store
        .history
        .apply(envelope(Command::AddLayer {
            id: "layer-1".to_string(),
            name: "layer-1".to_string(),
            width: 2.0,
            height: 1.0,
            index: 0,
            x: Some(0.0),
            y: Some(0.0),
            layer_type: None,
            shape_params: None,
            text_data: None,
        }))
        .expect("add layer");
    store.init_layer("layer-1", 2, 1, pixels.to_vec());
    store
        .history
        .apply(envelope(Command::SetAdjustment {
            id: "layer-1".to_string(),
            adjustment: Some(BasicAdjustment {
                brightness: 25.0,
                contrast: 0.0,
                saturation: 0.0,
            }),
        }))
        .expect("adjustment set");
    reg
}

const PRE_BAKE: [u8; 8] = [10, 20, 30, 40, 50, 60, 70, 80];
const BAKED: [u8; 8] = [90, 200, 255, 255, 100, 220, 255, 255];

#[test]
fn baked_adjustment_replaces_canonical_pixels_and_one_undo_restores_both() {
    let mut reg = seeded_store_with(&PRE_BAKE);
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::ApplyBasicAdjustment {
            id: "layer-1".to_string(),
            pixels: Some(RasterizePixels {
                width: 2.0,
                height: 1.0,
                rgba: BAKED.to_vec(),
            }),
        }))
        .expect("apply adjustment");

    {
        let layer = &reg.docs.get("d").unwrap().layers["layer-1"];
        assert_eq!(
            layer.pixels, BAKED,
            "the bake replaces the canonical buffer"
        );
    }
    {
        let metas = &reg.docs.get("d").unwrap().history.snapshot().layers;
        assert!(
            metas[0].basic_adjustment.is_none(),
            "bake drops the metadata"
        );
        assert_eq!(metas[0].has_adjustments, Some(false));
    }

    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::Undo))
        .expect("undo the bake");

    let layer = &reg.docs.get("d").unwrap().layers["layer-1"];
    assert_eq!(
        layer.pixels, PRE_BAKE,
        "one undo restores the pre-bake pixels"
    );
    let metas = &reg.docs.get("d").unwrap().history.snapshot().layers;
    assert!(
        metas[0].basic_adjustment.is_some(),
        "one undo restores the adjustment metadata too"
    );
}

#[test]
fn apply_basic_adjustment_with_zero_adjustment_drops_metadata_without_touching_pixels() {
    let mut reg = seeded_store_with(&PRE_BAKE);
    {
        let store = reg.docs.get_mut("d").unwrap();
        store
            .history
            .apply(envelope(Command::SetAdjustment {
                id: "layer-1".to_string(),
                adjustment: Some(BasicAdjustment {
                    brightness: 0.0,
                    contrast: 0.0,
                    saturation: 0.0,
                }),
            }))
            .expect("zero adjustment set");
    }
    reg.docs
        .get_mut("d")
        .unwrap()
        .apply_command(envelope(Command::ApplyBasicAdjustment {
            id: "layer-1".to_string(),
            pixels: None,
        }))
        .expect("apply zero adjustment");
    let layer = &reg.docs.get("d").unwrap().layers["layer-1"];
    assert_eq!(
        layer.pixels, PRE_BAKE,
        "a zero-adjustment bake writes no pixels"
    );
    let metas = &reg.docs.get("d").unwrap().history.snapshot().layers;
    assert!(metas[0].basic_adjustment.is_none());
    assert_eq!(metas[0].has_adjustments, Some(false));
}
