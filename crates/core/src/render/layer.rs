// SPDX-License-Identifier: AGPL-3.0-or-later
// Render layer value types: `RenderLayer` (the layer DTO the engine and host
// both speak), its optional dirty `Rect`, and the `RenderLayerChange` delta.
// Render-owned only: this module must not depend on `document` or on the wire
// projection types (see `crates/core/CONTEXT.md`).

use crate::ptz::content::{BasicAdjustment, BlendMode, LayerType, ShapeParams, TextData};
use crate::wire::command::ResourceId;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderLayer {
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub opacity: f64,
    pub resource_id: ResourceId,
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
    pub dirty_rect: Option<Rect>,
    // Canonical metadata subset (Option so old-shape envelopes still parse and
    // so the engine can carry only the fields an add/merge op sets). These
    // mirror the canonical model's string-enum + bool + f64 fields verbatim;
    // `width`/`height` are the new layer's dimensions when set by an add op.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_type: Option<LayerType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend_mode: Option<BlendMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_transparency: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_position: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_rotation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_background: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_adjustments: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    // Horizontal/vertical flip flags (mirror of Transform2D.flipH/flipV). Optional
    // so an add/upsert envelope without them parses; when set by the TransformLayer
    // arm they project onto the shadow (D-b: Some(v) overrides the canonical base).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flip_h: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flip_v: Option<bool>,
    // Nested parametric payloads (mirror of PtzLayer.shapeParams/textData/
    // basicAdjustment). Optional so v2 envelopes without them still parse. The
    // typed-add / SetLayerParams / SetAdjustment arms set these; the merge bridge
    // takes a Some (Some(v) overrides the canonical base, None preserves it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape_params: Option<ShapeParams>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_data: Option<TextData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basic_adjustment: Option<BasicAdjustment>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[allow(clippy::large_enum_variant)] // Upsert carries the full RenderLayer by value for O(1) history swap
pub enum RenderLayerChange {
    Upsert {
        layer: RenderLayer,
    },
    // DeleteLayer: carries resourceId so a future Resource
    // Registry can drive lifecycle (release/retain) WITHOUT re-owning pixels.
    #[serde(rename_all = "camelCase")]
    Remove {
        id: String,
        resource_id: ResourceId,
    },
}
