use super::*;
use std::rc::Rc;

fn target() -> BrushTarget {
    BrushTarget {
        doc_id: "doc".into(),
        layer_id: "layer".into(),
        width: 1,
        height: 1,
        display_scale: 1.0,
        is_background: false,
    }
}

fn stroke() -> BrushStroke {
    brush::begin_stroke(
        target(),
        0.0,
        0.0,
        4.0,
        false,
        1.0,
        Rc::new(brush::build_tip(4.0, 1.0, [255, 0, 0, 255])),
    )
}

#[test]
fn gesture_lifecycle_reaches_cleanup_and_idle() {
    let mut input = InputState::new();
    assert_eq!(input.phase(), GesturePhase::Idle);
    assert!(input.begin_brush(stroke()));
    assert_eq!(input.phase(), GesturePhase::Action);
    assert!(input.update_brush(2.0, 0.0));
    let stroke = input.finish_brush().expect("action phase commits");
    assert!(stroke.dabs.len() >= 2, "the move must add dabs");
    assert_eq!(input.phase(), GesturePhase::Committed);
    assert!(input.cleanup());
    assert_eq!(input.phase(), GesturePhase::Idle);
}

#[test]
fn out_of_order_transitions_are_refused() {
    let mut input = InputState::new();
    assert!(!input.cleanup(), "cleanup cannot run from idle");
    assert!(input.finish_brush().is_none(), "idle has no stroke");
    assert!(input.begin_brush(stroke()));
    assert!(!input.begin_brush(stroke()), "a second down is refused");
    assert_eq!(input.phase(), GesturePhase::Action);
}

#[test]
fn cancel_drops_the_stroke_and_allows_a_new_one() {
    let mut input = InputState::new();
    assert!(input.begin_brush(stroke()));
    assert!(input.update_brush(2.0, 0.0));
    input.cancel();
    assert_eq!(input.phase(), GesturePhase::Idle);
    assert!(!input.has_active_brush());
    assert!(
        input.begin_brush(stroke()),
        "a new gesture starts after cancel"
    );
}

#[test]
fn cursor_policy_covers_all_tools() {
    for (tool, expected) in [
        (ToolId::Move, CursorPolicy::SystemDefault),
        (ToolId::Selection, CursorPolicy::Crosshair),
        (ToolId::Crop, CursorPolicy::Crosshair),
        (ToolId::Eyedropper, CursorPolicy::Crosshair),
        (ToolId::Brush, CursorPolicy::SystemDefault),
        (ToolId::Eraser, CursorPolicy::SystemDefault),
        (ToolId::PaintBucket, CursorPolicy::Crosshair),
        (ToolId::Gradient, CursorPolicy::Crosshair),
        (ToolId::Shape, CursorPolicy::Crosshair),
        (ToolId::Text, CursorPolicy::Text),
    ] {
        assert_eq!(
            cursor_policy_for_tool(tool),
            expected,
            "cursor for {tool:?}"
        );
    }
}

#[test]
fn cursor_branch_table_covers_reachable_states() {
    use CursorPolicy::*;
    use GesturePhase::{Action, Idle};
    let context = |tool, phase, layer_locked, layer_paintable| CursorContext {
        tool,
        phase,
        layer_locked,
        layer_paintable,
        dragging: false,
        cloning: false,
    };
    // Alt held during a Move drag is the clone affordance; Alt on its own, with
    // no gesture running, changes nothing.
    let alt = |tool, phase, cloning| CursorContext {
        tool,
        phase,
        layer_locked: false,
        layer_paintable: true,
        dragging: false,
        cloning,
    };
    let drag = |tool, alt_held| CursorContext {
        tool,
        phase: GesturePhase::Idle,
        layer_locked: false,
        layer_paintable: true,
        dragging: true,
        cloning: alt_held,
    };
    for (ctx, expected) in [
        (alt(ToolId::Move, Action, true), Cloning),
        (alt(ToolId::Move, Idle, true), SystemDefault),
        (alt(ToolId::Selection, Action, true), Crosshair),
        (drag(ToolId::Move, true), Cloning),
        (drag(ToolId::Move, false), Grabbing),
        (drag(ToolId::Selection, true), Crosshair),
        (context(ToolId::Move, Idle, false, true), SystemDefault),
        (context(ToolId::Move, Action, false, true), Grabbing),
        // Moving a locked layer keeps the system default, not the block cursor.
        (context(ToolId::Move, Idle, true, true), SystemDefault),
        (context(ToolId::Selection, Idle, false, true), Crosshair),
        (context(ToolId::Crop, Idle, false, true), Crosshair),
        (context(ToolId::Eyedropper, Idle, false, true), Crosshair),
        (context(ToolId::Brush, Idle, false, true), SystemDefault),
        (context(ToolId::Eraser, Idle, false, true), SystemDefault),
        (context(ToolId::PaintBucket, Idle, false, true), Crosshair),
        (context(ToolId::Gradient, Idle, false, true), Crosshair),
        (context(ToolId::Shape, Idle, false, true), Crosshair),
        (context(ToolId::Text, Idle, false, true), Text),
        // Write tools refuse a locked or non-raster layer.
        (context(ToolId::Brush, Idle, true, true), NotAllowed),
        (context(ToolId::Eraser, Action, true, true), NotAllowed),
        (context(ToolId::PaintBucket, Idle, false, false), NotAllowed),
        (context(ToolId::Gradient, Idle, true, true), NotAllowed),
        // Read-only and layer-creating tools are not gated by the selection.
        (context(ToolId::Eyedropper, Idle, true, true), Crosshair),
        (context(ToolId::Selection, Action, true, true), Crosshair),
        (context(ToolId::Shape, Idle, true, false), Crosshair),
        (context(ToolId::Text, Idle, true, true), Text),
    ] {
        assert_eq!(
            cursor_policy_for_context(ctx),
            expected,
            "cursor for {ctx:?}"
        );
    }
}

#[test]
fn scaled_preview_renders_the_scaled_tip_not_its_corner() {
    let size = 1000.0;
    let stroke = brush::begin_stroke(
        BrushTarget {
            doc_id: "doc".into(),
            layer_id: "layer".into(),
            width: 1024,
            height: 1024,
            display_scale: 0.1,
            is_background: false,
        },
        512.0,
        512.0,
        size,
        false,
        1.0,
        Rc::new(brush::build_tip(size, 1.0, [255, 0, 0, 255])),
    );
    let layer = vec![0; 1024 * 1024 * 4];
    let patches = scaled_preview_patches(
        PreviewBase::document(&layer, 1024, 1024),
        &stroke.dabs,
        &stroke.tip,
        stroke.eraser,
        stroke.target.display_scale,
        256,
    )
    .expect("preview succeeds");
    let patch = patches
        .iter()
        .find(|patch| {
            512 >= patch.doc_x
                && 512 < patch.doc_x + patch.doc_width
                && 512 >= patch.doc_y
                && 512 < patch.doc_y + patch.doc_height
        })
        .expect("the dab centre is inside a patch");
    assert!(patch.image_width <= 256 && patch.image_height <= 256);
    let centre = preview_pixel(patch, 512, 512);
    assert!(
        centre[3] > 200,
        "the dab centre must be painted, got alpha {}",
        centre[3]
    );
    // A full-disc preview is transparent here; sampling only the tip's
    // top-left corner (the pre-fix behaviour) leaves this pixel opaque.
    let corner = preview_pixel(patch, 910, 910);
    assert_eq!(
        corner[3], 0,
        "the stamp corner must stay outside the disc, got alpha {}",
        corner[3]
    );
}

fn preview_pixel(patch: &ScaledPreviewPatch, doc_x: u32, doc_y: u32) -> [u8; 4] {
    let image_x =
        ((doc_x - patch.doc_x) as f64 * patch.image_width as f64 / patch.doc_width as f64).floor();
    let image_y = ((doc_y - patch.doc_y) as f64 * patch.image_height as f64
        / patch.doc_height as f64)
        .floor();
    let index = (image_y as usize * patch.image_width as usize + image_x as usize) * 4;
    [
        patch.data[index],
        patch.data[index + 1],
        patch.data[index + 2],
        patch.data[index + 3],
    ]
}

#[test]
fn tip_cache_reuses_the_same_settings_and_rebuilds_on_change() {
    let mut cache = BrushTipCache::default();
    let first = cache.get(40.0, 0.8, [255, 0, 0, 255]);
    let again = cache.get(40.0, 0.8, [255, 0, 0, 255]);
    assert!(Rc::ptr_eq(&first, &again));
    let other_size = cache.get(41.0, 0.8, [255, 0, 0, 255]);
    assert!(!Rc::ptr_eq(&first, &other_size));
    let other_color = cache.get(41.0, 0.8, [0, 255, 0, 255]);
    assert!(!Rc::ptr_eq(&other_size, &other_color));
}

#[test]
fn scaled_preview_respects_cap_and_document_bounds() {
    let mut stroke = brush::begin_stroke(
        BrushTarget {
            doc_id: "doc".into(),
            layer_id: "layer".into(),
            width: 8,
            height: 8,
            display_scale: 0.5,
            is_background: false,
        },
        2.0,
        2.0,
        4.0,
        false,
        1.0,
        Rc::new(brush::build_tip(4.0, 1.0, [255, 0, 0, 255])),
    );
    stroke.dabs.push(RasterDab {
        x: 4.5,
        y: 4.5,
        alpha: 1.0,
    });
    stroke.dabs.push(RasterDab {
        x: 5.5,
        y: 5.0,
        alpha: 1.0,
    });
    let layer = vec![0; 8 * 8 * 4];
    let patches = scaled_preview_patches(
        PreviewBase::document(&layer, 8, 8),
        &stroke.dabs,
        &stroke.tip,
        stroke.eraser,
        stroke.target.display_scale,
        256,
    )
    .expect("preview succeeds");
    assert!(!patches.is_empty(), "preview must produce patches");
    for patch in patches {
        assert!(patch.image_width <= 256 && patch.image_height <= 256);
        assert!(patch.doc_width >= 1 && patch.doc_height >= 1);
        assert!(patch.doc_x + patch.doc_width <= 8);
        assert!(patch.doc_y + patch.doc_height <= 8);
        assert_eq!(
            patch.data.len(),
            patch.image_width as usize * patch.image_height as usize * 4
        );
    }
}

#[test]
fn region_preview_matches_the_document_preview_for_the_same_crop() {
    let mut stroke = brush::begin_stroke(
        BrushTarget {
            doc_id: "doc".into(),
            layer_id: "layer".into(),
            width: 64,
            height: 64,
            display_scale: 0.5,
            is_background: false,
        },
        30.0,
        30.0,
        6.0,
        false,
        1.0,
        Rc::new(brush::build_tip(6.0, 1.0, [255, 0, 0, 255])),
    );
    stroke.dabs.push(RasterDab {
        x: 34.5,
        y: 31.0,
        alpha: 1.0,
    });
    let mut document = Vec::with_capacity(64 * 64 * 4);
    for _ in 0..64 * 64 {
        document.extend_from_slice(&[10, 20, 30, 255]);
    }
    let rect = preview_rect(&stroke.dabs, &stroke.tip, 64, 64)
        .expect("rect computes")
        .expect("the stroke has dabs");
    assert!(rect.x + rect.width <= 64 && rect.y + rect.height <= 64);

    let whole = scaled_preview_patches(
        PreviewBase::document(&document, 64, 64),
        &stroke.dabs,
        &stroke.tip,
        stroke.eraser,
        stroke.target.display_scale,
        256,
    )
    .expect("document preview");
    let mut region = Vec::with_capacity(rect.width as usize * rect.height as usize * 4);
    for y in rect.y..rect.y + rect.height {
        let start = ((y as usize * 64) + rect.x as usize) * 4;
        let end = start + rect.width as usize * 4;
        region.extend_from_slice(&document[start..end]);
    }
    let crop = scaled_preview_patches(
        PreviewBase::region(&region, rect, 64, 64),
        &stroke.dabs,
        &stroke.tip,
        stroke.eraser,
        stroke.target.display_scale,
        256,
    )
    .expect("region preview");

    assert!(!whole.is_empty(), "the stroke must produce preview pixels");
    assert_eq!(whole.len(), crop.len());
    for (document_patch, region_patch) in whole.iter().zip(&crop) {
        assert_eq!(
            (
                document_patch.doc_x,
                document_patch.doc_y,
                document_patch.doc_width,
                document_patch.doc_height
            ),
            (
                region_patch.doc_x,
                region_patch.doc_y,
                region_patch.doc_width,
                region_patch.doc_height
            )
        );
        assert_eq!(
            (document_patch.image_width, document_patch.image_height),
            (region_patch.image_width, region_patch.image_height)
        );
        assert_eq!(document_patch.data, region_patch.data);
    }
}

#[test]
fn region_preview_rejects_a_base_that_does_not_cover_the_crop() {
    let stroke = brush::begin_stroke(
        BrushTarget {
            doc_id: "doc".into(),
            layer_id: "layer".into(),
            width: 64,
            height: 64,
            display_scale: 1.0,
            is_background: false,
        },
        32.0,
        32.0,
        6.0,
        false,
        1.0,
        Rc::new(brush::build_tip(6.0, 1.0, [255, 0, 0, 255])),
    );
    let slim = vec![0u8; 4 * 4 * 4];
    let error = scaled_preview_patches(
        PreviewBase::region(
            &slim,
            PreviewRect {
                x: 0,
                y: 0,
                width: 4,
                height: 4,
            },
            64,
            64,
        ),
        &stroke.dabs,
        &stroke.tip,
        stroke.eraser,
        stroke.target.display_scale,
        256,
    )
    .expect_err("a 4x4 base cannot cover a stroke near the document centre");
    assert_eq!(error, "preview base does not cover the stroke crop");
}
