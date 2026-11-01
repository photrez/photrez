// SPDX-License-Identifier: AGPL-3.0-or-later
// The persisted LAYER content: the layer type union, the blend-mode union, and
// the layer record itself.
//
// `PtzDocumentContent` (the parent module) owns the document-level content;
// this module owns everything that describes ONE layer.

use super::Transform2D;
use crate::ptz::layer_params::{BasicAdjustment, ShapeParams, TextData};
use crate::wire::command::ResourceId;
use serde::{Deserialize, Serialize};

/// The layer `type` union (5 values).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayerType {
    Raster,
    Adjustment,
    Group,
    Shape,
    Text,
}

/// The 12 blend modes, with the exact strings saved documents carry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    #[serde(rename = "color-dodge")]
    ColorDodge,
    #[serde(rename = "color-burn")]
    ColorBurn,
    #[serde(rename = "soft-light")]
    SoftLight,
    #[serde(rename = "hard-light")]
    HardLight,
    Difference,
    Exclusion,
}

/// PtzLayer: every persisted layer field, TYPED.
/// Pixels are NOT stored: raster pixel references (imageBitmap /
/// baseImageBitmap) are represented by `resource_id` (a token handle, mirroring
/// RenderLayer.resource_id). `bitmapEpoch` is intentionally excluded (transient).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtzLayer {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub layer_type: LayerType,
    pub visible: bool,
    pub opacity: f64,
    pub locked: bool,
    #[serde(default)]
    pub is_background: Option<bool>,
    #[serde(default)]
    pub lock_transparency: Option<bool>,
    #[serde(default)]
    pub lock_position: Option<bool>,
    #[serde(default)]
    pub lock_rotation: Option<bool>,
    #[serde(default)]
    pub has_adjustments: Option<bool>,
    #[serde(default)]
    pub basic_adjustment: Option<BasicAdjustment>,
    /// Token handle to the layer's pixel resource. Mirrors RenderLayer.resource_id.
    /// None for non-raster layers. Replaces imageBitmap / baseImageBitmap.
    /// Absent in v3 saved files (which carry no resource tokens); deserializes to None.
    #[serde(default)]
    pub resource_id: Option<ResourceId>,
    pub blend_mode: BlendMode,
    pub transform: Transform2D,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub shape_params: Option<ShapeParams>,
    #[serde(default)]
    pub text_data: Option<TextData>,
}
