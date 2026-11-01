// SPDX-License-Identifier: AGPL-3.0-or-later
// Value bridge between the persisted `.ptz` layer model and the render-side
// `RenderLayer`. Three directions live here:
//   * down-projection (`to_render_layer`): the minimal layer the compositor
//     consumes;
//   * up-projection (`project_known_layer` / `project_new_layer`): every shared
//     metadata field, for a layer the engine already knows or is about to learn;
//   * merge (`merge_into_layer`): renderer-driven edits folded back onto a
//     persisted layer without touching the persisted-only fields.
//
// The replica in `ptz::replica` depends on this module; this module never
// depends on the replica (see `crates/core/CONTEXT.md`).

use crate::ptz::content::{LayerType, PtzLayer, Transform2D};
use crate::render::layer::RenderLayer;
use crate::wire::command::ResourceId;

/// Project a PtzLayer down to the minimal render layer the compositor
/// consumes. Only the fields the renderer drives (identity, visibility,
/// opacity, pixel-resource handle, transform) are carried. `dirty_rect` is a
/// per-frame transport hint the persisted model does not own, so it is always
/// `None` here. Returns a NEW value (COW: never mutates the input).
pub fn to_render_layer(c: &PtzLayer) -> RenderLayer {
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
        // so the persisted-only metadata subset is deliberately left unset.
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

/// Project a `PtzLayer` into a `RenderLayer` carrying EVERY shared
/// metadata field (typed enums, locks, dims, flips, nested params). The
/// persisted push is the authoritative document state at restore time, so the
/// projection must not silently drop fields the engine's arms can hold -
/// dropping them would make the NEXT delta restatement reset consumer-side
/// state to defaults. `resource_id` and `dirty_rect` are NOT set here: they
/// are engine-owned (see the two wrappers below).
fn project_fields(c: &PtzLayer) -> RenderLayer {
    RenderLayer {
        id: c.id.clone(),
        name: c.name.clone(),
        visible: c.visible,
        opacity: c.opacity,
        resource_id: 0, // placeholder: overwritten by the wrappers below
        x: c.transform.x,
        y: c.transform.y,
        scale_x: c.transform.scale_x,
        scale_y: c.transform.scale_y,
        rotation: c.transform.rotation,
        dirty_rect: None, // engine-owned: preserved (known) or cleared (new)
        layer_type: Some(c.layer_type.clone()),
        blend_mode: Some(c.blend_mode.clone()),
        locked: Some(c.locked),
        // Explicit like `locked`: only the SetLocked arm writes these, so an
        // engine `None` would make an undo back to "unlocked" indistinguishable
        // from "no opinion" and the shadow would keep the stale lock.
        lock_transparency: Some(c.lock_transparency.unwrap_or(false)),
        lock_position: Some(c.lock_position.unwrap_or(false)),
        lock_rotation: Some(c.lock_rotation.unwrap_or(false)),
        is_background: c.is_background,
        // Always explicit: an omitted flag (`None`) in the persisted layer is
        // still a definite "unadjusted" fact, and leaving it None would make a
        // later SetAdjustment's pre-state ambiguous — its undo could not tell
        // "never adjusted" from "engine has no opinion".
        has_adjustments: Some(c.basic_adjustment.is_some() || c.has_adjustments == Some(true)),
        width: Some(c.width),
        height: Some(c.height),
        flip_h: Some(c.transform.flip_h),
        flip_v: Some(c.transform.flip_v),
        shape_params: c.shape_params.clone(),
        text_data: c.text_data.clone(),
        basic_adjustment: c.basic_adjustment.clone(),
    }
}

/// Project a `PtzLayer` into the engine's `RenderLayer` for a layer the
/// engine ALREADY knows about (id present in the native set). Carries the full
/// shared metadata (see `project_fields`) while preserving the ENGINE's
/// existing `resource_id` and `dirty_rect`: the renderer cannot own either
/// (the engine minted the pixel-resource handle and owns the per-frame dirty
/// hint). The `to_render_layer` 0-sentinel semantics are left untouched.
pub fn project_known_layer(c: &PtzLayer, engine: &RenderLayer) -> RenderLayer {
    let mut r = project_fields(c);
    r.resource_id = engine.resource_id;
    r.dirty_rect = engine.dirty_rect.clone();
    r
}

/// Project a `PtzLayer` for a layer the engine does NOT yet know about
/// (a document-side layer arriving via a persisted re-push). Mints a fresh,
/// non-zero `resource_id` supplied by the caller (the engine's `next_resource`),
/// and starts the layer with no dirty hint. Every other shared field carries
/// the full persisted metadata.
pub fn project_new_layer(c: &PtzLayer, minted_resource_id: ResourceId) -> RenderLayer {
    let mut r = project_fields(c);
    r.resource_id = minted_resource_id;
    r
}

fn explicit_clear(r: &RenderLayer) -> bool {
    r.has_adjustments == Some(false) && r.basic_adjustment.is_none()
}

fn base_was_never_adjusted(base: &PtzLayer) -> bool {
    base.basic_adjustment.is_none() && base.has_adjustments != Some(true)
}

/// Merge one partial-lock flag. The engine carries an explicit opinion for
/// these fields after projection, so an explicit `false` is a real unlock (the
/// undo walker restoring the pre-toggle side) and must reach the shadow. A
/// persisted layer that never carried the key keeps it absent, so an unlock
/// does not invent a `false` the file never had.
fn merge_lock(r: Option<bool>, base: Option<bool>) -> Option<bool> {
    match (r, base) {
        (Some(false), None) => None,
        (Some(v), _) => Some(v),
        (None, base) => base,
    }
}

/// Project renderer-driven edits from a RenderLayer back onto a PtzLayer
/// without touching any persisted-only content. Returns a NEW layer (COW: the
/// base is never mutated in place). The renderer cannot express the 14
/// persisted-only fields (layer type, blend mode, dimensions, shapes, text,
/// adjustments), so those are preserved verbatim from `base`. The lock flags
/// are the exception: only the SetLocked arm writes them, so their merge rule
/// carries the engine's explicit opinion (see `merge_lock`). A
/// `resource_id` of 0 on the render layer maps back to `None` (no pixel
/// resource); every other field is taken from the render layer.
///
/// Note: there is deliberately NO `PtzLayer::from(RenderLayer)`. That
/// would force fabricating defaults for the 14 persisted-only fields, which is
/// a silent-data-loss trap. Build a full model seed when real data exists.
pub fn merge_into_layer(base: &PtzLayer, r: &RenderLayer) -> PtzLayer {
    let layer_type = r
        .layer_type
        .clone()
        .unwrap_or_else(|| base.layer_type.clone());
    // A raster layer's engine opinion is always "no parametric payload" (the
    // compositor ignores them there), and the None-preserves merge below
    // cannot express `apply_rasterize`'s payload drop (None means "unchanged"
    // there). So a Raster projection whose render layer carries neither
    // payload drops both for good; a render layer that explicitly
    // carries one (SetLayerParams on a raster layer) still keeps it.
    let parametric = !matches!(layer_type, LayerType::Raster)
        || r.shape_params.is_some()
        || r.text_data.is_some();
    PtzLayer {
        id: r.id.clone(),
        name: r.name.clone(),
        // Extended fields: a Some on the render layer (set by an add/upsert op)
        // overrides the persisted base; None preserves it. Enums use the
        // match-and-clone pattern so a missing value falls back to `base`.
        layer_type,
        visible: r.visible,
        opacity: r.opacity,
        locked: r.locked.unwrap_or(base.locked),
        is_background: r.is_background.or(base.is_background),
        lock_transparency: merge_lock(r.lock_transparency, base.lock_transparency),
        lock_position: merge_lock(r.lock_position, base.lock_position),
        lock_rotation: merge_lock(r.lock_rotation, base.lock_rotation),
        // Nested parametric payloads: a Some on the render layer (set by the typed
        // AddLayer / SetLayerParams / SetAdjustment arm) overrides the persisted
        // base; None preserves it (D-b: Some(v) takes, None preserves).
        has_adjustments: if base_was_never_adjusted(base) && explicit_clear(r) {
            // The engine always carries an explicit flag, but a persisted layer
            // that never had an adjustment omits it. Keep the absent form so a
            // save does not invent `hasAdjustments: false` the file never had.
            base.has_adjustments
        } else {
            r.has_adjustments.or(base.has_adjustments)
        },
        basic_adjustment: if explicit_clear(r) {
            // An explicit clear (SetAdjustment { None } or an undo back to the
            // unadjusted state) must remove the persisted adjustment; the plain
            // None-preserves rule would resurrect it forever.
            None
        } else {
            r.basic_adjustment
                .clone()
                .or_else(|| base.basic_adjustment.clone())
        },
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
        shape_params: if parametric {
            r.shape_params.clone().or_else(|| base.shape_params.clone())
        } else {
            None
        },
        text_data: if parametric {
            r.text_data.clone().or_else(|| base.text_data.clone())
        } else {
            None
        },
    }
}
