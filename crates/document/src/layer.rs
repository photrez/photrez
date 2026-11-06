// SPDX-License-Identifier: AGPL-3.0-or-later
// The document-owned layer metadata value, its optional dirty `Rect`, and the
// `LayerChange` delta the engine emits for one applied command. The engine
// stores these in the COW `LayerSet` and in history; the compositor reads them.
// Most fields are `Option` because an add or merge delta carries only what the
// command set, and because envelopes from before those fields existed must
// still parse.

use crate::content::{BasicAdjustment, BlendMode, LayerType, ShapeParams, TextData};
use crate::identity::ResourceId;
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
pub struct LayerMeta {
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
    // Canonical metadata subset. These mirror the persisted model's string-enum
    // + bool + f64 fields verbatim; `width`/`height` are the new layer's
    // dimensions when set by an add op.
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
    // Flip flags mirror `Transform2D.flip_h`/`flip_v`. `Some(v)` overrides the
    // canonical base, `None` preserves it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flip_h: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flip_v: Option<bool>,
    // Nested parametric payloads mirror `PtzLayer`. A set value overrides the
    // canonical base, `None` preserves it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape_params: Option<ShapeParams>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_data: Option<TextData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basic_adjustment: Option<BasicAdjustment>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[allow(clippy::large_enum_variant)] // Upsert carries the full LayerMeta by value for O(1) history swap
pub enum LayerChange {
    Upsert {
        layer: LayerMeta,
    },
    // DeleteLayer: carries resourceId so a future Resource
    // Registry can drive lifecycle (release/retain) WITHOUT re-owning pixels.
    #[serde(rename_all = "camelCase")]
    Remove {
        id: String,
        resource_id: ResourceId,
    },
}
