// SPDX-License-Identifier: AGPL-3.0-or-later
// ADDITIVE, UNWIRED: the RenderLayer <-> CanonicalLayer value bridge and the
// native seed-payload validation surface. Nothing in production consumes these
// yet; they back the gated native-authority shadow seed and the TS builder
// contract test. Mirror of canonical_model.rs's additive/unwired posture.

use crate::canonical_model::{CanonicalDocument, CanonicalLayer, Transform2D};
use crate::model::RenderLayer;
use std::collections::HashMap;

/// Project a CanonicalLayer down to the minimal render layer the compositor
/// consumes. Only the fields the renderer drives (identity, visibility,
/// opacity, pixel-resource handle, transform) are carried. `dirty_rect` is a
/// per-frame transport hint the canonical model does not own, so it is always
/// `None` here. Returns a NEW value (COW: never mutates the input).
pub fn render_layer_from_canonical(c: &CanonicalLayer) -> RenderLayer {
    RenderLayer {
        id: c.id.clone(),
        name: c.name.clone(),
        visible: c.visible,
        opacity: c.opacity,
        resource_id: c.resource_id.unwrap_or(0),
        x: c.transform.x,
        y: c.transform.y,
        scale_x: c.transform.scale_x,
        scale_y: c.transform.scale_y,
        rotation: c.transform.rotation,
        dirty_rect: None,
        // Down-projection: the renderer drives only transform/opacity/visibility,
        // so the canonical-only metadata subset is deliberately left unset.
        layer_type: None,
        blend_mode: None,
        locked: None,
        lock_transparency: None,
        lock_position: None,
        lock_rotation: None,
        is_background: None,
        has_adjustments: None,
        width: None,
        height: None,
        flip_h: None,
        flip_v: None,
        shape_params: None,
        text_data: None,
        basic_adjustment: None,
    }
}

/// Project renderer-driven edits from a RenderLayer back onto a CanonicalLayer
/// without touching any canonical-only content. Returns a NEW layer (COW: the
/// base is never mutated in place). The renderer cannot express the 14
/// canonical-only fields (layer type, blend mode, locks, dimensions, shapes,
/// text, adjustments), so those are preserved verbatim from `base`. A
/// `resource_id` of 0 on the render layer maps back to `None` (no pixel
/// resource); every other field is taken from the render layer.
///
/// Note: there is deliberately NO `CanonicalLayer::from(RenderLayer)`. That
/// would force fabricating defaults for the 14 canonical-only fields, which is
/// a silent-data-loss trap. Build a full model seed when real data exists.
pub fn merge_render_layer_into_canonical(base: &CanonicalLayer, r: &RenderLayer) -> CanonicalLayer {
    CanonicalLayer {
        id: r.id.clone(),
        name: r.name.clone(),
        // Extended fields: a Some on the render layer (set by an add/upsert op)
        // overrides the canonical base; None preserves it. Enums use the
        // match-and-clone pattern so a missing value falls back to `base`.
        layer_type: r
            .layer_type
            .clone()
            .unwrap_or_else(|| base.layer_type.clone()),
        visible: r.visible,
        opacity: r.opacity,
        locked: r.locked.unwrap_or(base.locked),
        is_background: r.is_background.or(base.is_background),
        lock_transparency: r.lock_transparency.or(base.lock_transparency),
        lock_position: r.lock_position.or(base.lock_position),
        lock_rotation: r.lock_rotation.or(base.lock_rotation),
        has_adjustments: r.has_adjustments.or(base.has_adjustments),
        // Nested parametric payloads: a Some on the render layer (set by the typed
        // AddLayer / SetLayerParams / SetAdjustment arm) overrides the canonical
        // base; None preserves it (D-b: Some(v) takes, None preserves).
        basic_adjustment: r
            .basic_adjustment
            .clone()
            .or_else(|| base.basic_adjustment.clone()),
        resource_id: if r.resource_id == 0 {
            None
        } else {
            Some(r.resource_id)
        },
        blend_mode: r
            .blend_mode
            .clone()
            .unwrap_or_else(|| base.blend_mode.clone()),
        transform: Transform2D {
            x: r.x,
            y: r.y,
            scale_x: r.scale_x,
            scale_y: r.scale_y,
            rotation: r.rotation,
            flip_h: r.flip_h.unwrap_or(base.transform.flip_h),
            flip_v: r.flip_v.unwrap_or(base.transform.flip_v),
        },
        width: r.width.unwrap_or(base.width),
        height: r.height.unwrap_or(base.height),
        shape_params: r.shape_params.clone().or_else(|| base.shape_params.clone()),
        text_data: r.text_data.clone().or_else(|| base.text_data.clone()),
    }
}

/// Per-document canonical shadow kept in sync with engine layer edits via
/// reconciliation (not re-built from scratch each edit). Holds a full
/// `CanonicalDocument` copy plus tombstones for deleted layers and an `incomplete`
/// flag that marks engine-minted layers whose 14 canonical-only fields cannot be
/// reconstructed from the render layer.
#[derive(Debug)]
pub struct CanonicalShadow {
    pub doc: CanonicalDocument,
    // Tombstones for deleted layers; `pub(crate)` so intra-crate shadow tests can
    // assert on delete/undo restore behavior.
    pub(crate) removed: HashMap<String, CanonicalLayer>,
    pub incomplete: bool,
}

impl CanonicalShadow {
    /// Wrap a freshly pushed document. Tombstones empty, `incomplete` false: a
    /// full push discards any prior delete history and unknown-layer marks.
    pub fn new(doc: CanonicalDocument) -> Self {
        CanonicalShadow {
            doc,
            removed: HashMap::new(),
            incomplete: false,
        }
    }

    /// Reconcile the shadow against the current engine layer set. Engine order IS
    /// canonical order, so `doc.layers` is rebuilt in a single pass over `current`:
    ///  - id in `doc.layers` -> merge render edits onto the existing layer,
    ///  - id in a tombstone -> restore it (merged with current render state),
    ///  - id in neither -> engine-minted layer: skipped, `incomplete` is set.
    /// After the pass, any `doc.layers` entry whose id is absent from `current`
    /// (deleted) is moved to a tombstone so an undo-of-delete restores it with all
    /// fields intact. O(layers), metadata-only, no serialization.
    pub fn reconcile<'a, I: Iterator<Item = &'a RenderLayer>>(&mut self, current: I) {
        // Re-index existing canonical layers by id; matched ones are drained as they
        // are rebuilt, so whatever remains at the end is a deleted layer.
        let mut existing: HashMap<String, CanonicalLayer> = std::mem::take(&mut self.doc.layers)
            .into_iter()
            .map(|l| (l.id.clone(), l))
            .collect();

        let mut rebuilt: Vec<CanonicalLayer> = Vec::new();
        for r in current {
            if let Some(base) = existing.remove(&r.id) {
                rebuilt.push(merge_render_layer_into_canonical(&base, r));
            } else if let Some(tomb) = self.removed.remove(&r.id) {
                // Ids are host-minted random values; a restored tombstone reuses
                // its original id, which is benign-by-design (the restored layer is
                // the same logical layer the engine deleted). Id collision with an
                // unrelated fresh layer is negligible.
                rebuilt.push(merge_render_layer_into_canonical(&tomb, r));
            } else {
                // Engine-minted layer: cannot reconstruct canonical-only fields.
                // Leave it out and flag the shadow incomplete (sticky).
                self.incomplete = true;
                continue;
            }
        }

        // Any layer left in `existing` was removed from the engine set: move it to a
        // tombstone so a future undo can restore its full fields.
        for (id, layer) in existing {
            self.removed.insert(id, layer);
        }

        self.doc.layers = rebuilt;
    }
}

/// Validation surface for the native seed payload contract: confirms `json`
/// parses into a `CanonicalDocument` (the exact shape the TS builder
/// `buildCanonicalDocumentPayload` emits). Returns an empty string on success or
/// the serde error message on failure. Pure parse-check only - it performs no
/// persistence and stores nothing. Shared by the wasm export and the native test.
pub fn canonical_validate_payload(json: &str) -> String {
    match serde_json::from_str::<CanonicalDocument>(json) {
        Ok(_) => String::new(),
        Err(e) => e.to_string(),
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn canonical_validate_json(json: &str) -> String {
    canonical_validate_payload(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical_model::*;
    use crate::model::Rect;

    // Duplicated from canonical_model's model test module: the bridge tests
    // below call raster_layer()/shape_layer(), which live there. Kept tiny and
    // identical to avoid cross-module test-visibility coupling.
    fn sample_transform() -> Transform2D {
        Transform2D {
            x: 10.0,
            y: 20.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 15.0,
            flip_h: false,
            flip_v: true,
        }
    }

    fn raster_layer() -> CanonicalLayer {
        CanonicalLayer {
            id: "layer-raster".to_string(),
            name: "Background".to_string(),
            layer_type: LayerType::Raster,
            visible: true,
            opacity: 1.0,
            locked: false,
            is_background: Some(true),
            lock_transparency: Some(false),
            lock_position: None,
            lock_rotation: None,
            has_adjustments: Some(true),
            basic_adjustment: Some(BasicAdjustment {
                brightness: 5.0,
                contrast: -10.0,
                saturation: 20.0,
            }),
            resource_id: Some(1),
            blend_mode: BlendMode::Normal,
            transform: sample_transform(),
            width: 800.0,
            height: 600.0,
            shape_params: None,
            text_data: None,
        }
    }

    fn shape_layer() -> CanonicalLayer {
        CanonicalLayer {
            id: "layer-shape".to_string(),
            name: "Star 1".to_string(),
            layer_type: LayerType::Shape,
            visible: true,
            opacity: 0.8,
            locked: false,
            is_background: None,
            lock_transparency: None,
            lock_position: None,
            lock_rotation: None,
            has_adjustments: None,
            basic_adjustment: None,
            resource_id: None,
            blend_mode: BlendMode::Multiply,
            transform: sample_transform(),
            width: 200.0,
            height: 200.0,
            shape_params: Some(ShapeParams {
                kind: ShapeKind::Star,
                width: 200.0,
                height: 200.0,
                radius: 12.0,
                fill: ShapeFill {
                    kind: ShapeFillKind::Solid,
                    color: "#E15A17".to_string(),
                },
                stroke: ShapeStroke {
                    enabled: true,
                    color: "#000000".to_string(),
                    width: 2.0,
                },
                arrow_head: false,
            }),
            text_data: None,
        }
    }

    fn fully_populated_layer() -> CanonicalLayer {
        CanonicalLayer {
            id: "canon-id".to_string(),
            name: "canon-name".to_string(),
            layer_type: LayerType::Text,
            visible: true,
            opacity: 0.55,
            locked: true,
            is_background: Some(true),
            lock_transparency: Some(true),
            lock_position: Some(true),
            lock_rotation: Some(true),
            has_adjustments: Some(true),
            basic_adjustment: Some(BasicAdjustment {
                brightness: 1.0,
                contrast: 2.0,
                saturation: 3.0,
            }),
            resource_id: Some(9),
            blend_mode: BlendMode::Overlay,
            transform: Transform2D {
                x: 11.0,
                y: 22.0,
                scale_x: 1.5,
                scale_y: 2.5,
                rotation: 33.0,
                flip_h: true,
                flip_v: false,
            },
            width: 700.0,
            height: 500.0,
            shape_params: Some(ShapeParams {
                kind: ShapeKind::Heart,
                width: 90.0,
                height: 80.0,
                radius: 5.0,
                fill: ShapeFill {
                    kind: ShapeFillKind::Solid,
                    color: "#ABCDEF".to_string(),
                },
                stroke: ShapeStroke {
                    enabled: false,
                    color: "#000000".to_string(),
                    width: 0.0,
                },
                arrow_head: true,
            }),
            text_data: Some(TextData {
                content: "Z".to_string(),
                font_family: "Tahoma".to_string(),
                font_size: 24.0,
                font_weight: 300.0,
                font_style: TextFontStyle::Normal,
                color: "#123456".to_string(),
                align: TextAlign::Right,
                line_height: 1.0,
                letter_spacing: 2.0,
                box_mode: TextBoxMode::Point,
                box_width: 10.0,
                box_height: 20.0,
                stroke: TextStroke {
                    width: 1.0,
                    color: "#654321".to_string(),
                    align: Some(TextStrokeAlign::Inside),
                },
                underline: Some(true),
                strikethrough: Some(false),
                uppercase: Some(false),
            }),
        }
    }

    fn distinct_render_layer() -> RenderLayer {
        RenderLayer {
            id: "render-id".to_string(),
            name: "render-name".to_string(),
            visible: false,
            opacity: 0.12,
            resource_id: 99,
            x: 100.0,
            y: 200.0,
            scale_x: 3.0,
            scale_y: 4.0,
            rotation: 45.0,
            dirty_rect: Some(Rect {
                x: 1,
                y: 1,
                width: 2,
                height: 2,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn render_layer_mapping_uses_distinct_overlap_values() {
        // Every overlap field gets a unique, recognizable value so a swap would
        // be caught by the assertions below.
        let c = CanonicalLayer {
            id: "c-id".to_string(),
            name: "c-name".to_string(),
            layer_type: LayerType::Shape,
            visible: false,
            opacity: 0.42,
            locked: true,
            is_background: Some(false),
            lock_transparency: Some(true),
            lock_position: None,
            lock_rotation: None,
            has_adjustments: Some(false),
            basic_adjustment: None,
            resource_id: Some(7),
            blend_mode: BlendMode::Screen,
            transform: Transform2D {
                x: 1.0,
                y: 2.0,
                scale_x: 3.0,
                scale_y: 4.0,
                rotation: 5.0,
                flip_h: true,
                flip_v: false,
            },
            width: 100.0,
            height: 120.0,
            shape_params: Some(ShapeParams {
                kind: ShapeKind::Star,
                width: 50.0,
                height: 60.0,
                radius: 7.0,
                fill: ShapeFill {
                    kind: ShapeFillKind::Solid,
                    color: "#FFFFFF".to_string(),
                },
                stroke: ShapeStroke {
                    enabled: true,
                    color: "#000000".to_string(),
                    width: 1.0,
                },
                arrow_head: true,
            }),
            text_data: None,
        };

        let r = render_layer_from_canonical(&c);
        assert_eq!(r.id, "c-id");
        assert_eq!(r.name, "c-name");
        assert_eq!(r.visible, false);
        assert_eq!(r.opacity, 0.42);
        assert_eq!(r.resource_id, 7);
        assert_eq!(r.x, 1.0);
        assert_eq!(r.y, 2.0);
        assert_eq!(r.scale_x, 3.0);
        assert_eq!(r.scale_y, 4.0);
        assert_eq!(r.rotation, 5.0);
        assert_eq!(
            r.dirty_rect, None,
            "render layer bridge never sets dirty_rect"
        );
    }

    #[test]
    fn merge_round_trip_preserves_canonical_with_resource() {
        let mut base = raster_layer();
        base.resource_id = Some(42);
        let r = render_layer_from_canonical(&base);
        assert_eq!(r.resource_id, 42);
        let merged = merge_render_layer_into_canonical(&base, &r);
        assert_eq!(merged, base, "round-trip must be value-identical");
    }

    #[test]
    fn merge_round_trip_preserves_canonical_without_resource() {
        let base = shape_layer();
        assert_eq!(base.resource_id, None);
        let r = render_layer_from_canonical(&base);
        assert_eq!(r.resource_id, 0, "absent resource maps to 0");
        let merged = merge_render_layer_into_canonical(&base, &r);
        assert_eq!(merged, base, "None -> 0 -> None must round-trip");
    }

    #[test]
    fn merge_preserves_all_canonical_only_fields() {
        let c = fully_populated_layer();
        let r = distinct_render_layer();
        let m = merge_render_layer_into_canonical(&c, &r);

        // Overlap fields come from the render layer.
        assert_eq!(m.id, "render-id");
        assert_eq!(m.name, "render-name");
        assert_eq!(m.visible, false);
        assert_eq!(m.opacity, 0.12);
        assert_eq!(m.resource_id, Some(99));
        assert_eq!(m.transform.x, 100.0);
        assert_eq!(m.transform.y, 200.0);
        assert_eq!(m.transform.scale_x, 3.0);
        assert_eq!(m.transform.scale_y, 4.0);
        assert_eq!(m.transform.rotation, 45.0);

        // The 14 canonical-only fields survive untouched (no silent loss).
        assert_eq!(m.layer_type, c.layer_type);
        assert_eq!(m.locked, c.locked);
        assert_eq!(m.is_background, c.is_background);
        assert_eq!(m.lock_transparency, c.lock_transparency);
        assert_eq!(m.lock_position, c.lock_position);
        assert_eq!(m.lock_rotation, c.lock_rotation);
        assert_eq!(m.has_adjustments, c.has_adjustments);
        assert_eq!(m.basic_adjustment, c.basic_adjustment);
        assert_eq!(m.blend_mode, c.blend_mode);
        assert_eq!(m.transform.flip_h, c.transform.flip_h);
        assert_eq!(m.transform.flip_v, c.transform.flip_v);
        assert_eq!(m.width, c.width);
        assert_eq!(m.height, c.height);
        assert_eq!(m.shape_params, c.shape_params);
        assert_eq!(m.text_data, c.text_data);
    }

    #[test]
    fn merge_takes_render_layer_nested_params_when_present() {
        // D-b ratchet: an extended Some on the render layer (set by the typed
        // AddLayer / SetLayerParams / SetAdjustment arms) overrides the canonical
        // base; a None on the render layer preserves the base value.
        let base = shape_layer();
        let mut r = render_layer_from_canonical(&base);
        let new_params = ShapeParams {
            kind: ShapeKind::Heart,
            width: 90.0,
            height: 80.0,
            radius: 5.0,
            fill: ShapeFill {
                kind: ShapeFillKind::Solid,
                color: "#ABCDEF".to_string(),
            },
            stroke: ShapeStroke {
                enabled: false,
                color: "#000000".to_string(),
                width: 0.0,
            },
            arrow_head: true,
        };
        r.shape_params = Some(new_params.clone());
        // text_data / basic_adjustment left None on the render layer -> base preserved.
        let m = merge_render_layer_into_canonical(&base, &r);
        assert_eq!(m.shape_params, Some(new_params), "render Some must take");
        assert_eq!(m.text_data, base.text_data, "render None preserves base");
        assert_eq!(
            m.basic_adjustment, base.basic_adjustment,
            "render None preserves base"
        );
    }

    #[test]
    fn dirty_rect_does_not_leak_into_canonical() {
        let base = raster_layer();
        let r_with = RenderLayer {
            id: "r1".to_string(),
            name: "n1".to_string(),
            visible: true,
            opacity: 0.5,
            resource_id: 3,
            x: 5.0,
            y: 6.0,
            scale_x: 2.0,
            scale_y: 2.0,
            rotation: 9.0,
            dirty_rect: Some(Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            }),
            ..Default::default()
        };
        let r_without = RenderLayer {
            id: "r1".to_string(),
            name: "n1".to_string(),
            visible: true,
            opacity: 0.5,
            resource_id: 3,
            x: 5.0,
            y: 6.0,
            scale_x: 2.0,
            scale_y: 2.0,
            rotation: 9.0,
            dirty_rect: None,
            ..Default::default()
        };

        let m_with = merge_render_layer_into_canonical(&base, &r_with);
        let m_without = merge_render_layer_into_canonical(&base, &r_without);
        assert_eq!(
            m_with, m_without,
            "dirty_rect has no canonical counterpart and must be ignored"
        );
        // Transform reflects only the overlap fields, not the dirty rect.
        assert_eq!(m_with.transform.x, 5.0);
        assert_eq!(m_with.transform.scale_x, 2.0);
        assert_eq!(m_with.transform.rotation, 9.0);
        assert_eq!(m_with.transform.flip_h, base.transform.flip_h);
        assert_eq!(m_with.transform.flip_v, base.transform.flip_v);
    }
}

#[cfg(test)]
mod canonical_validate_tests {
    use super::*;

    const VALID_DOC: &str = r##"{
        "id":"v-1",
        "name":"Valid",
        "width":800,
        "height":600,
        "layers":[
            {
                "id":"L1","name":"T","type":"text","visible":true,"opacity":1,"locked":false,
                "blendMode":"normal",
                "transform":{"x":1,"y":2,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},
                "width":100,"height":20,
                "textData":{"content":"Hi","fontFamily":"Arial","fontSize":32,"fontWeight":400,"fontStyle":"normal","color":"#000000","align":"left","lineHeight":1.2,"letterSpacing":0,"boxMode":"point","boxWidth":0,"boxHeight":0,"stroke":{"width":0,"color":"#000000","align":"outside"},"underline":false,"strikethrough":false,"uppercase":false}
            }
        ]
    }"##;

    #[test]
    fn valid_payload_returns_empty_string() {
        // Returns "" (ok) - the contract the TS builder must satisfy.
        assert_eq!(canonical_validate_payload(VALID_DOC), "");
    }

    #[test]
    fn malformed_payload_returns_error_string() {
        let err = canonical_validate_payload("{ not valid json");
        assert!(!err.is_empty(), "malformed json must surface a serde error");
    }
}
