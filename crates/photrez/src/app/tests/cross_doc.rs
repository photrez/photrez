//! Cross-document layer drag and file-drop routing.
//!
//! The GPUI drag transport (pointer-down on a row, drop hit-test) is not
//! drivable from the mounted harness; these tests pin the production handler
//! that every drop target calls with the payload GPUI would deliver.

use super::*;
use crate::app::cross_doc::{LayerDragPayload, LayerDropTarget};
use gpui_kit::test::TestWindowExt;
use photrez_core::ptz::content::{PtzDocumentContent, PtzLayer};

const FIXTURE_DOC: &str = "parity-doc";
const FIXTURE_PNG: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/bounded-raster.layer.png"
);

fn content(app: &Entity<App>, cx: &gpui_kit::App, doc_id: &str) -> PtzDocumentContent {
    app.read(cx)
        .sessions
        .get(doc_id)
        .expect("the document is open")
        .loaded_document()
        .content_snapshot()
        .expect("the document has content")
        .clone()
}

fn layer_count(app: &Entity<App>, cx: &gpui_kit::App, doc_id: &str) -> usize {
    content(app, cx, doc_id).layers.len()
}

fn copied_layer<'a>(before: &[PtzLayer], after: &'a [PtzLayer]) -> &'a PtzLayer {
    after
        .iter()
        .find(|layer| !before.iter().any(|known| known.id == layer.id))
        .expect("the drop creates one layer")
}

#[gpui_kit::test]
fn cross_document_layer_drag_copies_pixels_and_state(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, app, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let source_ids = app.read(cx).active_core_layer_ids_for_test();
        let source_id = source_ids.first().cloned().expect("fixture layer");
        let source_pixels = app
            .read(cx)
            .core_layer_pixels_for_test(&source_id)
            .expect("fixture layer pixels");
        let source_before = content(&app, cx, FIXTURE_DOC).layers;

        app.update(cx, |app, cx| {
            app.create_document(8, 8, cx)
                .expect("blank target document");
        });
        window.render_frame(cx);
        let target_doc = app
            .read(cx)
            .open_tab_ids_for_test()
            .last()
            .cloned()
            .expect("target tab");
        let target_before = content(&app, cx, &target_doc).layers;
        app.update(cx, |app, cx| {
            app.layer_drag_impl(
                &LayerDragPayload {
                    source_doc_id: FIXTURE_DOC.to_string(),
                    core_id: Some(source_id.clone()),
                    row_id: 0,
                },
                LayerDropTarget::Tab {
                    doc_id: target_doc.clone(),
                },
                false,
                cx,
            )
            .expect("cross-document copy");
        });
        window.render_frame(cx);

        let target_after = content(&app, cx, &target_doc).layers;
        assert_eq!(target_after.len(), target_before.len() + 1);
        let copied = copied_layer(&target_before, &target_after);
        assert_eq!(copied.width, source_before[0].width);
        assert_eq!(copied.height, source_before[0].height);
        assert_eq!(copied.opacity, source_before[0].opacity);
        assert_eq!(copied.blend_mode, source_before[0].blend_mode);
        assert_eq!(copied.visible, source_before[0].visible);
        // A tab drop has no canvas cursor, so it centers in the target.
        assert_eq!(
            copied.transform.x,
            ((8.0_f64 - copied.width) / 2.0).max(0.0)
        );
        assert_eq!(
            copied.transform.y,
            ((8.0_f64 - copied.height) / 2.0).max(0.0)
        );
        let copied_pixels = app
            .read(cx)
            .core_layer_pixels_for_test(&copied.id)
            .expect("the copy owns pixels");
        assert_eq!(
            (copied_pixels.0, copied_pixels.1),
            (source_pixels.0, source_pixels.1)
        );
        assert_eq!(copied_pixels.2, source_pixels.2);

        // The copy leaves the source document untouched.
        assert_eq!(content(&app, cx, FIXTURE_DOC).layers, source_before);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn alt_layer_drag_moves_and_guards_the_last_source_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, app, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let fixture_layer = app
            .read(cx)
            .active_core_layer_ids_for_test()
            .first()
            .cloned()
            .expect("fixture layer");
        app.update(cx, |app, cx| {
            app.create_document(8, 8, cx)
                .expect("blank target document");
        });
        window.render_frame(cx);
        let target_doc = app
            .read(cx)
            .open_tab_ids_for_test()
            .last()
            .cloned()
            .expect("target tab");

        // The fixture owns a single layer; ALT must refuse to empty it.
        let refusal = app.update(cx, |app, cx| {
            app.layer_drag_impl(
                &LayerDragPayload {
                    source_doc_id: FIXTURE_DOC.to_string(),
                    core_id: Some(fixture_layer),
                    row_id: 0,
                },
                LayerDropTarget::Tab {
                    doc_id: target_doc.clone(),
                },
                true,
                cx,
            )
        });
        assert_eq!(
            refusal,
            Err("Cannot move the last layer from a document. Drop cancelled.".to_string())
        );
        assert_eq!(layer_count(&app, cx, FIXTURE_DOC), 1);

        // Move the fixture's duplicate into the target document.
        app.update(cx, |app, cx| {
            app.activate_document(FIXTURE_DOC, cx)
                .expect("the source document reopens");
        });
        window.render_frame(cx);
        let moved_id = app.update(cx, |app, cx| {
            app.duplicate_selected_layer_id(cx)
                .expect("the fixture layer can be duplicated")
        });
        window.render_frame(cx);
        assert_eq!(layer_count(&app, cx, FIXTURE_DOC), 2);
        app.update(cx, |app, cx| {
            app.layer_drag_impl(
                &LayerDragPayload {
                    source_doc_id: FIXTURE_DOC.to_string(),
                    core_id: Some(moved_id),
                    row_id: 0,
                },
                LayerDropTarget::Tab {
                    doc_id: target_doc.clone(),
                },
                true,
                cx,
            )
            .expect("ALT moves the layer");
        });
        window.render_frame(cx);
        assert_eq!(layer_count(&app, cx, FIXTURE_DOC), 1);
        assert_eq!(layer_count(&app, cx, &target_doc), 2);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn layer_drag_on_the_empty_workspace_seeds_a_document(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, app, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let source_layer = content(&app, cx, FIXTURE_DOC).layers[0].clone();
        let tabs_before = app.read(cx).open_tab_ids_for_test();
        app.update(cx, |app, cx| {
            app.layer_drag_impl(
                &LayerDragPayload {
                    source_doc_id: FIXTURE_DOC.to_string(),
                    core_id: Some(source_layer.id.clone()),
                    row_id: 0,
                },
                LayerDropTarget::NewDocument,
                false,
                cx,
            )
            .expect("empty-workspace drop seeds a document");
        });
        window.render_frame(cx);
        let tabs = app.read(cx).open_tab_ids_for_test();
        assert_eq!(tabs.len(), tabs_before.len() + 1);
        let new_doc = tabs.last().cloned().expect("new document tab");
        let seeded = content(&app, cx, &new_doc);
        assert_eq!(
            (seeded.width, seeded.height),
            (source_layer.width, source_layer.height)
        );
        // The seeded background plus the copied layer.
        assert_eq!(seeded.layers.len(), 2);
        let copied = seeded
            .layers
            .iter()
            .find(|layer| layer.id != seeded.layers[0].id || layer.name == source_layer.name)
            .expect("copied layer");
        assert_eq!(copied.transform.x, 0.0);
        assert_eq!(copied.transform.y, 0.0);
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn file_drop_adds_centered_layers_with_the_cascade_offset(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, app, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let dropped = directory.path().join("dropped.png");
        std::fs::copy(FIXTURE_PNG, &dropped).expect("fixture image is copied");
        let before = content(&app, cx, FIXTURE_DOC).layers;
        let added = app.update(cx, |app, cx| {
            app.drop_files_as_layers(
                &[dropped.clone(), dropped],
                FIXTURE_DOC,
                (100.0, 120.0),
                true,
                cx,
            )
        });
        assert_eq!(added, Ok(2));
        window.render_frame(cx);
        let after = content(&app, cx, FIXTURE_DOC).layers;
        assert_eq!(after.len(), before.len() + 2);
        let mut dropped: Vec<&PtzLayer> = after
            .iter()
            .filter(|layer| !before.iter().any(|known| known.id == layer.id))
            .collect();
        dropped.sort_by(|a, b| a.transform.x.total_cmp(&b.transform.x));
        let first = dropped[0];
        let second = dropped[1];
        assert_eq!(first.transform.x, 100.0 - first.width / 2.0);
        assert_eq!(first.transform.y, 120.0 - first.height / 2.0);
        assert_eq!(second.transform.x, first.transform.x + 24.0);
        assert_eq!(second.transform.y, first.transform.y + 24.0);
        assert_eq!(first.name, "dropped.png");
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn layer_drag_refuses_a_full_target_document(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, app, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let source_layer = content(&app, cx, FIXTURE_DOC).layers[0].clone();
        app.update(cx, |app, cx| {
            app.create_document(1, 1, cx)
                .expect("blank target document");
        });
        window.render_frame(cx);
        let target_doc = app
            .read(cx)
            .open_tab_ids_for_test()
            .last()
            .cloned()
            .expect("target tab");
        let mut next_id = 0;
        while layer_count(&app, cx, &target_doc) < crate::app::cross_doc::MAX_LAYERS {
            next_id += 1;
            let id = format!("fill-{next_id}");
            app.update(cx, |app, cx| {
                app.seed_layer(&target_doc, &id, 1, 1, vec![0, 0, 0, 0])
                    .expect("fill pixels");
                app.apply_doc_command(
                    &target_doc,
                    photrez_core::protocol::Command::AddLayer {
                        id,
                        name: "fill".to_string(),
                        width: 1.0,
                        height: 1.0,
                        index: 0,
                        x: None,
                        y: None,
                        layer_type: Some(photrez_core::ptz::content::LayerType::Raster),
                        shape_params: None,
                        text_data: None,
                    },
                    cx,
                )
                .expect("fill layer");
            });
        }
        let refusal = app.update(cx, |app, cx| {
            app.layer_drag_impl(
                &LayerDragPayload {
                    source_doc_id: FIXTURE_DOC.to_string(),
                    core_id: Some(source_layer.id),
                    row_id: 0,
                },
                LayerDropTarget::Tab {
                    doc_id: target_doc.clone(),
                },
                false,
                cx,
            )
        });
        assert_eq!(
            refusal,
            Err(format!(
                "Target document reached max {} layers",
                crate::app::cross_doc::MAX_LAYERS
            ))
        );
        assert_eq!(
            layer_count(&app, cx, &target_doc),
            crate::app::cross_doc::MAX_LAYERS
        );
    })
    .expect("test window remains open");
}
