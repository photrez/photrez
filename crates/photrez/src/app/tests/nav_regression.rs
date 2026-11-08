//! Mounted regression tests for the native shell.
//!
//! Mounts the real view tree in a headless gpui-kit test window and drives it
//! through real clicks, drags, and measured element bounds: tool-rail order and
//! selection, option-bar mounting, inspector and layer tab bars and their panel
//! bodies, dock-column geometry, layer-row selection and reorder, the dock state
//! path, and startup rejection of an unpresentable document.
//!
//! Partial coverage, not a closed surface: the right dock follows
//! `AppState.dock_open`, but no title-bar toggle is mounted.
//!
//! Explicit gaps: interactive document tabs, the tool-variant fly-out,
//! interactive top-level menus, a title-bar dock toggle, and command-bus
//! dialog entry points. A delete-layer confirmation has no test because the
//! app offers none. The document-tab strip and the top-level
//! menu triggers are also absent from the test access tree because their views
//! do not call `.test_support()`; asserting them would require a production
//! edit outside this lane. The native views remain visual-only or reachable
//! only through a `--shot` state,
//! or expose no corresponding production event; no handler or confirmation is
//! fabricated. These checks do not measure pixels, present interval,
//! input-to-photon latency, or smoothness.
//!
//! Needs `gpui-kit/test-support` (enabled from `[dev-dependencies]`).

use gpui_kit::base::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    px, size, AppContext, Bounds, Entity, Point, TestAppContext, WindowBounds, WindowHandle,
    WindowOptions,
};
use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::app::App;
use crate::services::error::ServiceError;
use crate::services::file_io::{FileIo, SaveTarget};
use crate::state::{InspectorTab, LayerRow, LayerTab, SelectionShape, ToolId};
use crate::views;

/// Mount the production root in a deterministic 1366x820 test window.
fn open_app(cx: &mut TestAppContext) -> (WindowHandle<Root>, Entity<App>) {
    open_app_with(cx, None)
}

fn open_app_with(
    cx: &mut TestAppContext,
    startup: Option<(PathBuf, Arc<dyn FileIo>)>,
) -> (WindowHandle<Root>, Entity<App>) {
    open_app_sized(cx, startup, 1366.0, 820.0)
}

fn open_app_sized(
    cx: &mut TestAppContext,
    startup: Option<(PathBuf, Arc<dyn FileIo>)>,
    width: f32,
    height: f32,
) -> (WindowHandle<Root>, Entity<App>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        views::title_bar::init(&cx.svg_renderer());
        crate::app::load_fonts(cx);
    });

    cx.update(|cx| {
        let bounds = Bounds {
            origin: Point::default(),
            size: size(px(width), px(height)),
        };
        let (window, content) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                cx.new(|cx| match startup.as_ref() {
                    Some((path, file_io)) => App::new_with_file_io(
                        window,
                        None,
                        Some(path.clone()),
                        Arc::clone(file_io),
                        cx,
                    ),
                    None => App::new(window, None, None, cx),
                })
            },
        )
        .expect("open test window");
        (window.downcast::<Root>().expect("Base Root"), content)
    })
}

/// The active tab's underline must not move the label: active and inactive
/// labels sit on one baseline, the way the reference draws its underline as an
/// absolute overlay.
///
/// RED proof: against the underline-as-last-child tab this fails on a one
/// pixel rise in the active label's centre.
#[gpui_kit::test]
fn active_tab_label_sits_on_the_inactive_baseline(cx: &mut TestAppContext) {
    let (handle, content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.inspector_tab = crate::state::InspectorTab::Adjustments;
            });
        });
        window.render_frame(cx);

        let active = window.find("adjustments-label").bounds().center().y;
        let inactive = window.find("properties-label").bounds().center().y;
        assert!(
            (active - inactive).abs() < px(0.5),
            "the active label must not shift: active {active:?} vs inactive {inactive:?}"
        );
    })
    .unwrap();
}

/// Clicking the brush button must move the real `AppState` while a document
/// is open, and answer nothing without one - the rail paints at 50% and loses
/// its clicks over the empty workspace, the Layers panel's own treatment.
///
/// The starting tool and the document flag are written through the state
/// entity so the test does not depend on the process-wide shot seed
/// (`shot.rs` sets it per test binary).
///
/// RED proof: re-attach the handlers unconditionally in `views/tool_rail.rs`
/// and the first assert fails on the tool the inert click switched to.
#[gpui_kit::test]
fn tool_rail_click_updates_active_tool(cx: &mut TestAppContext) {
    let (handle, content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let state = content.read(cx).state_for_test().clone();
        state.update(cx, |s, _| {
            s.active_tool = ToolId::Move;
            s.has_document = false;
        });
        window.render_frame(cx);
        assert_eq!(
            state.read(cx).active_tool,
            ToolId::Move,
            "the test must start from Move regardless of the shot seed"
        );

        window.click("brush", cx);
        window.render_frame(cx);
        assert_eq!(
            state.read(cx).active_tool,
            ToolId::Move,
            "without a document the rail click must be inert"
        );

        state.update(cx, |s, _| s.has_document = true);
        window.render_frame(cx);
        window.click("brush", cx);
        window.render_frame(cx);

        assert_eq!(
            state.read(cx).active_tool,
            ToolId::Brush,
            "clicking the brush button must update active_tool"
        );
    })
    .unwrap();
}

/// Clicking a dock tab must move the real `AppState`.
///
/// Both tab bars are seeded deterministically through the state entity
/// (`Properties` / `Layers`), then every tab id is clicked. The click order is
/// chosen so each click actually changes the selection; otherwise a click on
/// the already-selected tab would pass even with the handler removed.
///
/// RED proof: before the dock's click handlers exist, the selection never
/// moves off the seed.
#[gpui_kit::test]
fn dock_tabs_click_update_selected_tab(cx: &mut TestAppContext) {
    let (handle, content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let state = content.read(cx).state_for_test().clone();
        state.update(cx, |s, _| {
            s.inspector_tab = InspectorTab::Properties;
            s.layer_tab = LayerTab::Layers;
        });
        window.render_frame(cx);

        // InspectorDock: Adjustments -> Presets -> Properties.
        let inspector_clicks = [
            ("adjustments", InspectorTab::Adjustments),
            ("presets", InspectorTab::Presets),
            ("properties", InspectorTab::Properties),
        ];
        for (id, expected) in inspector_clicks {
            window.click(id, cx);
            window.render_frame(cx);
            assert_eq!(
                state.read(cx).inspector_tab,
                expected,
                "clicking the {id} tab must update inspector_tab"
            );
        }

        // LayerDock: History -> Layers.
        let layer_clicks = [("history", LayerTab::History), ("layers", LayerTab::Layers)];
        for (id, expected) in layer_clicks {
            window.click(id, cx);
            window.render_frame(cx);
            assert_eq!(
                state.read(cx).layer_tab,
                expected,
                "clicking the {id} tab must update layer_tab"
            );
        }
    })
    .unwrap();
}

/// Dragging layer row 0 onto row 2 must reorder the live rows and select the
/// dragged row.
///
/// The rows are seeded through the real state entity, and both endpoints are
/// addressed by the production ids (`layer-row-0`, `layer-row-2`).
///
/// RED proof: with the row's drop handler removed, the order stays `[0, 1, 2]`
/// and the selection stays `2`, so both assertions fail. This is UI-state
/// wiring only; no undo/history is asserted because none exists yet.
#[gpui_kit::test]
fn layer_drag_reorders_and_selects(cx: &mut TestAppContext) {
    let (handle, content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let state = content.read(cx).state_for_test().clone();
        state.update(cx, |s, _| {
            // Other tests in this binary set the process-wide shot id, which
            // can seed the History tab; the rows only exist on Layers.
            s.layer_tab = LayerTab::Layers;
            s.layers = vec![
                LayerRow {
                    id: 0,
                    name: "Layer 1".into(),
                    core_id: None,
                },
                LayerRow {
                    id: 1,
                    name: "Layer 2".into(),
                    core_id: None,
                },
                LayerRow {
                    id: 2,
                    name: "Layer 3".into(),
                    core_id: None,
                },
            ];
            s.selected_layer = 2;
        });
        window.render_frame(cx);

        window.drag_to("layer-row-0", "layer-row-2", cx);
        window.render_frame(cx);

        let state = content.read(cx).state_for_test().clone();
        let s = state.read(cx);
        let ids: Vec<u64> = s.layers.iter().map(|l| l.id).collect();
        assert_eq!(
            ids,
            vec![1, 2, 0],
            "dropping row 0 onto row 2 must move it to row 2's old index"
        );
        assert_eq!(
            s.selected_layer, 0,
            "the dragged row must become the selected layer"
        );
    })
    .unwrap();
}

/// Clicking a layer row must move the real selection.
///
/// RED proof: remove the row's click handler and the selection stays on the
/// seeded row, so the assertion fails.
#[gpui_kit::test]
fn layer_row_click_selects_the_layer(cx: &mut TestAppContext) {
    let (handle, content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let state = content.read(cx).state_for_test().clone();
        state.update(cx, |s, _| {
            s.layer_tab = LayerTab::Layers;
            s.layers = vec![
                LayerRow {
                    id: 0,
                    name: "Layer 1".into(),
                    core_id: None,
                },
                LayerRow {
                    id: 1,
                    name: "Layer 2".into(),
                    core_id: None,
                },
                LayerRow {
                    id: 2,
                    name: "Layer 3".into(),
                    core_id: None,
                },
            ];
            s.selected_layer = 0;
        });
        window.render_frame(cx);

        window.click("layer-row-2", cx);
        window.render_frame(cx);

        assert_eq!(
            state.read(cx).selected_layer,
            2,
            "clicking a layer row must select that layer"
        );
    })
    .unwrap();
}

/// Each mounted dock tab must select and mount its matching panel body.
#[gpui_kit::test]
fn dock_tab_clicks_mount_only_selected_panel(cx: &mut TestAppContext) {
    let (handle, content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let state = content.read(cx).state_for_test().clone();
        state.update(cx, |s, _| {
            s.inspector_tab = InspectorTab::Properties;
            s.layer_tab = LayerTab::Layers;
        });
        window.render_frame(cx);

        let inspector_clicks = [
            (
                "adjustments",
                InspectorTab::Adjustments,
                "inspector-body-adjustments",
            ),
            ("presets", InspectorTab::Presets, "inspector-body-presets"),
            (
                "properties",
                InspectorTab::Properties,
                "inspector-body-properties",
            ),
        ];
        let inspector_bodies = [
            "inspector-body-properties",
            "inspector-body-adjustments",
            "inspector-body-presets",
        ];
        for (tab_id, expected, body_id) in inspector_clicks {
            window.click(tab_id, cx);
            window.render_frame(cx);
            assert_eq!(state.read(cx).inspector_tab, expected);
            assert!(window.find(body_id).visible(), "{body_id} must be mounted");
            for other_id in inspector_bodies {
                if other_id != body_id {
                    assert!(window.try_find(other_id).is_none());
                }
            }
        }

        let layer_clicks = [
            ("history", LayerTab::History, "layer-body-history"),
            ("layers", LayerTab::Layers, "layer-body-layers"),
        ];
        let layer_bodies = ["layer-body-layers", "layer-body-history"];
        for (tab_id, expected, body_id) in layer_clicks {
            window.click(tab_id, cx);
            window.render_frame(cx);
            assert_eq!(state.read(cx).layer_tab, expected);
            assert!(window.find(body_id).visible(), "{body_id} must be mounted");
            for other_id in layer_bodies {
                if other_id != body_id {
                    assert!(window.try_find(other_id).is_none());
                }
            }
        }
    })
    .unwrap();
}

/// Mounted dock columns and the Navigator must retain their measured bounds.
#[gpui_kit::test]
fn dock_columns_and_navigator_match_mounted_geometry(cx: &mut TestAppContext) {
    let (handle, _content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);

        let dock = window.find("right-dock").bounds();
        let inspector = window.find("inspector-dock").bounds();
        let layer = window.find("layer-dock").bounds();
        let navigator = window.find("navigator").bounds();

        assert_eq!(dock.size.width, px(533.));
        assert_eq!(inspector.size.width, px(272.));
        assert_eq!(layer.size.width, px(260.));
        assert_eq!(inspector.origin.x, dock.origin.x);
        assert_eq!(inspector.origin.y, dock.origin.y);
        assert_eq!(inspector.size.height, dock.size.height);
        assert_eq!(layer.origin.y, dock.origin.y);
        assert_eq!(layer.size.height, dock.size.height);
        // The measured columns total 532px within the 533px dock frame.
        assert_eq!(layer.origin.x, inspector.origin.x + inspector.size.width);
        assert_eq!(
            layer.origin.x + layer.size.width,
            dock.origin.x + dock.size.width - px(1.)
        );
        assert_eq!(navigator.origin.x, layer.origin.x);
        assert_eq!(navigator.size.width, layer.size.width);
        assert_eq!(navigator.size.height, px(185.));
        assert_eq!(
            navigator.origin.y + navigator.size.height,
            dock.origin.y + dock.size.height,
            "the Navigator must stay pinned to the dock bottom"
        );
    })
    .unwrap();
}

/// Every rail tool must select its matching option bar, with no other bar mounted.
#[gpui_kit::test]
fn each_tool_selects_its_option_bar(cx: &mut TestAppContext) {
    // The option bar is a document affordance: a launch that opens nothing
    // shows the empty workspace instead, so this mount opens a project.
    let paths = StartupTestFiles::new();
    let source = paths.file("tools.ptz");
    let io = Arc::new(StartupFileIo::default());
    io.insert(&source, startup_archive("doc-tools", 1, 1, false, 0));
    let (handle, content) = open_app_with(cx, Some((source, io)));
    cx.update_window(handle.into(), |_, window, cx| {
        let state = content.read(cx).state_for_test().clone();
        state.update(cx, |s, _| s.active_tool = ToolId::Text);
        window.render_frame(cx);

        let tools = [
            ("move", ToolId::Move, "option-bar-move"),
            ("selection", ToolId::Selection, "option-bar-selection"),
            ("crop", ToolId::Crop, "option-bar-crop"),
            ("eyedropper", ToolId::Eyedropper, "option-bar-eyedropper"),
            ("brush", ToolId::Brush, "option-bar-brush"),
            ("eraser", ToolId::Eraser, "option-bar-eraser"),
            (
                "paintBucket",
                ToolId::PaintBucket,
                "option-bar-paint-bucket",
            ),
            ("gradient", ToolId::Gradient, "option-bar-gradient"),
            ("shape", ToolId::Shape, "option-bar-shape"),
            ("text", ToolId::Text, "option-bar-text"),
        ];

        for pair in tools.windows(2) {
            assert!(
                window.find(pair[0].0).bounds().origin.y < window.find(pair[1].0).bounds().origin.y,
                "{} must appear above {} in the tool rail",
                pair[0].0,
                pair[1].0
            );
        }

        for (tool_id, expected_tool, option_bar_id) in tools {
            window.click(tool_id, cx);
            window.render_frame(cx);
            assert_eq!(state.read(cx).active_tool, expected_tool);
            assert!(
                window.find(option_bar_id).visible(),
                "selecting {tool_id} must mount {option_bar_id}"
            );
            for (_, _, other_id) in tools {
                if other_id != option_bar_id {
                    assert!(window.try_find(other_id).is_none());
                }
            }
        }
    })
    .unwrap();
}

/// The right dock follows `AppState.dock_open`.
///
/// This is the L9 state path only: no title-bar toggle is mounted, so the
/// "single toggle" half of L9 remains a GAP. The assertion still catches a
/// regression where dock visibility stops following the state field.
#[gpui_kit::test]
fn right_dock_visibility_follows_state(cx: &mut TestAppContext) {
    let (handle, content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let state = content.read(cx).state_for_test().clone();
        assert!(
            window.try_find("panel-toggle").is_none(),
            "the side-panel toggle is lg:hidden at the 1366px baseline"
        );
        state.update(cx, |s, _| {
            s.has_document = true;
            s.dock_open = true;
        });
        window.render_frame(cx);
        assert!(window.find("right-dock").visible());

        state.update(cx, |s, _| s.dock_open = false);
        window.render_frame(cx);
        for id in ["right-dock", "inspector-dock", "layer-dock"] {
            assert!(
                window.try_find(id).is_none(),
                "{id} must not mount while dock_open is false"
            );
        }

        state.update(cx, |s, _| s.dock_open = true);
        window.render_frame(cx);
        assert!(window.find("right-dock").visible());
    })
    .unwrap();
}

/// Each top-level trigger opens its own measured popup; an outside click closes it.
#[gpui_kit::test]
fn menu_triggers_open_measured_popups(cx: &mut TestAppContext) {
    let (handle, _content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let menus = [
            ("menus.file", "menu-file-popup", 58.0),
            ("menus.edit", "menu-edit-popup", 99.0),
            ("menus.image", "menu-image-popup", 142.0),
            ("menus.layer", "menu-layer-popup", 198.0),
            ("menus.view", "menu-view-popup", 249.0),
            ("menus.window", "menu-window-popup", 298.0),
            ("menus.help", "menu-help-popup", 367.0),
            ("menus.settings", "menu-settings-popup", 415.0),
        ];
        for (trigger, popup, x) in menus {
            window.click(trigger, cx);
            window.render_frame(cx);
            let bounds = window.find(popup).bounds();
            assert_eq!(bounds.size.width, px(224.), "{popup} width");
            assert_eq!(bounds.origin.y, px(36.), "{popup} top edge");
            assert_eq!(bounds.origin.x, px(x), "{popup} trigger origin");
            assert!(
                window.try_find("menu-overlay").is_some(),
                "the open menu must own the dismissal overlay"
            );

            window.click_at(
                "menu-overlay",
                Point {
                    x: px(1200.),
                    y: px(700.),
                },
                cx,
            );
            window.render_frame(cx);
            assert!(
                window.try_find(popup).is_none(),
                "an outside click must close {popup}"
            );
        }
    })
    .unwrap();
}

/// Menu commands open the three shell dialogs through the same event path.
#[gpui_kit::test]
fn menu_commands_open_the_shell_dialogs(cx: &mut TestAppContext) {
    let (handle, _content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let steps = [
            ("menus.file", "menu-new-document", "dialog-new-document"),
            ("menus.help", "menu-about", "dialog-about"),
            ("menus.settings", "menu-preferences", "dialog-settings"),
        ];
        for (trigger, row, dialog_id) in steps {
            window.click(trigger, cx);
            window.render_frame(cx);
            window.click(row, cx);
            window.render_frame(cx);
            assert!(window.find(dialog_id).visible(), "{dialog_id} must mount");
            assert!(
                window.try_find("menu-overlay").is_none(),
                "the menu must close when its command runs"
            );
            // The overlay fades in over 200ms; the test clock must pass it
            // before the overlay counts as visible for a click.
            cx.background_executor()
                .advance_clock(std::time::Duration::from_millis(250));
            window.render_frame(cx);

            window.click_at(
                "dialog-overlay",
                Point {
                    x: px(10.),
                    y: px(700.),
                },
                cx,
            );
            window.render_frame(cx);
            assert!(
                window.find(dialog_id).visible(),
                "{dialog_id} must stay open after an outside click"
            );
            window.click(
                match dialog_id {
                    "dialog-new-document" => "new-doc-cancel",
                    "dialog-about" => "about-close",
                    _ => "settings-ok",
                },
                cx,
            );
            window.render_frame(cx);
            assert!(
                window.try_find(dialog_id).is_none(),
                "{dialog_id} must close from its own control"
            );
        }
    })
    .unwrap();
}

/// Right-clicking the selection tool exposes its two sub-variants; choosing
/// one updates both the shape and the active tool. The rail answers the
/// pointer only while a document is open, so the state seeds one.
#[gpui_kit::test]
fn selection_variant_flyout_exposes_rect_and_ellipse(cx: &mut TestAppContext) {
    let (handle, content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let state = content.read(cx).state_for_test().clone();
        state.update(cx, |s, _| {
            s.active_tool = ToolId::Selection;
            s.selection_shape = SelectionShape::Rect;
            s.selection_flyout_open = false;
            s.has_document = true;
        });
        window.render_frame(cx);

        window.right_click("selection", cx);
        window.render_frame(cx);
        let flyout = window.find("selection-variant-flyout");
        assert!(flyout.visible(), "right-click must open the fly-out");
        assert_eq!(flyout.bounds().size.width, px(140.));
        assert_eq!(flyout.bounds().origin.x, px(50.));
        assert_eq!(flyout.bounds().origin.y, px(92.));
        assert!(window.find("variant-rect").visible());
        assert!(window.find("variant-ellipse").visible());

        window.click("variant-ellipse", cx);
        window.render_frame(cx);
        assert_eq!(state.read(cx).selection_shape, SelectionShape::Ellipse);
        assert_eq!(state.read(cx).active_tool, ToolId::Selection);
        assert!(
            window.try_find("selection-variant-flyout").is_none(),
            "choosing a variant must close the fly-out"
        );

        window.right_click("selection", cx);
        window.render_frame(cx);
        assert!(
            window.find("selection-variant-flyout").visible(),
            "the fly-out must reopen for the dismissal check"
        );
        window.click_at(
            "selection-flyout-frame",
            Point {
                x: px(600.),
                y: px(600.),
            },
            cx,
        );
        window.render_frame(cx);
        assert!(
            window.try_find("selection-variant-flyout").is_none(),
            "an outside click must dismiss the fly-out"
        );
    })
    .unwrap();
}

/// The tab strip shows the active document as the selected tab.
#[gpui_kit::test]
fn document_tab_shows_the_active_document(cx: &mut TestAppContext) {
    let paths = StartupTestFiles::new();
    let source = paths.file("active.ptz");
    let io = Arc::new(StartupFileIo::default());
    io.insert(&source, startup_archive("doc-active", 1, 1, false, 0));
    let (handle, _content) = open_app_with(cx, Some((source, io)));
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        let tab = window.find("document-tab-doc-active");
        assert!(tab.visible(), "the active document tab must be mounted");
        assert_eq!(tab.selected(), Some(true), "the tab must be the active one");
        assert_eq!(tab.label(), Some("Startup test"));
        assert!(
            window.try_find("document-tab-baseline").is_none(),
            "the seeded placeholder tab must not stand in for the active document"
        );
    })
    .unwrap();
}

/// Below the `lg` breakpoint the title-bar button is the dock's only toggle.
#[gpui_kit::test]
fn narrow_title_bar_toggle_controls_the_right_dock(cx: &mut TestAppContext) {
    let (handle, content) = open_app_sized(cx, None, 800.0, 600.0);
    cx.update_window(handle.into(), |_, window, cx| {
        let state = content.read(cx).state_for_test().clone();
        state.update(cx, |s, _| {
            s.has_document = true;
            s.dock_open = true;
        });
        window.render_frame(cx);
        assert!(window.find("panel-toggle").visible());
        assert!(window.find("right-dock").visible());

        window.click("panel-toggle", cx);
        window.render_frame(cx);
        assert!(!state.read(cx).dock_open);
        assert!(window.try_find("right-dock").is_none());

        window.click("panel-toggle", cx);
        window.render_frame(cx);
        assert!(state.read(cx).dock_open);
        assert!(window.find("right-dock").visible());
    })
    .unwrap();
}

#[derive(Default)]
struct StartupFileIo {
    files: Mutex<HashMap<PathBuf, Vec<u8>>>,
    approvals: AtomicUsize,
    reads: AtomicUsize,
}

impl StartupFileIo {
    fn insert(&self, path: &Path, bytes: Vec<u8>) {
        self.files.lock().unwrap().insert(path.to_owned(), bytes);
    }
}

impl FileIo for StartupFileIo {
    fn approve_path(&self, _raw_path: &Path) -> Result<(), ServiceError> {
        self.approvals.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn read_approved(&self, raw_path: &Path, max_bytes: u64) -> Result<Vec<u8>, ServiceError> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        let bytes = self
            .files
            .lock()
            .unwrap()
            .get(raw_path)
            .cloned()
            .ok_or_else(|| ServiceError::Validation("test path has no bytes".into()))?;
        if bytes.len() as u64 > max_bytes {
            return Err(ServiceError::ResourceLimit("test archive too large".into()));
        }
        Ok(bytes)
    }

    fn authorize_save_target(&self, _raw_path: &Path) -> Result<SaveTarget, ServiceError> {
        Err(ServiceError::Internal(
            "save authorization is unused".into(),
        ))
    }

    fn modified_ms(&self, _raw_path: &Path) -> Result<u64, ServiceError> {
        Err(ServiceError::Internal("timestamps are unused".into()))
    }
}

struct StartupTestFiles(PathBuf);

impl StartupTestFiles {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "photrez-startup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn file(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, []).unwrap();
        path
    }
}

impl Drop for StartupTestFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn startup_archive(
    id: &str,
    width: u32,
    height: u32,
    unsupported: bool,
    padding: usize,
) -> Vec<u8> {
    const PNG_1X1: [u8; 88] = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 1, 98, 58, 81, 161, 241, 31,
        0, 0, 0, 255, 255, 195, 173, 177, 143, 0, 0, 0, 6, 73, 68, 65, 84, 3, 0, 5, 230, 2, 106,
        179, 130, 221, 138, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let raster = |layer_id| {
        serde_json::json!({
            "id": layer_id,
            "name": "Layer",
            "type": "raster",
            "visible": true,
            "opacity": 1.0,
            "locked": false,
            "blendMode": "normal",
            "transform": {
                "x": 0.0,
                "y": 0.0,
                "scaleX": 1.0,
                "scaleY": 1.0,
                "rotation": 0.0,
                "flipH": false,
                "flipV": false
            },
            "width": width,
            "height": height
        })
    };
    let mut layers = vec![raster("layer-1")];
    if unsupported {
        layers.push(serde_json::json!({
            "id": "layer-2",
            "name": "Group",
            "type": "group",
            "visible": true,
            "opacity": 1.0,
            "locked": false,
            "blendMode": "normal",
            "transform": {
                "x": 0.0,
                "y": 0.0,
                "scaleX": 1.0,
                "scaleY": 1.0,
                "rotation": 0.0,
                "flipH": false,
                "flipV": false
            },
            "width": width,
            "height": height
        }));
    }
    let mut metadata = serde_json::to_vec(&serde_json::json!({
        "format": "photrez-ptz",
        "version": 3,
        "id": id,
        "name": "Startup test",
        "width": width,
        "height": height,
        "layers": layers
    }))
    .unwrap();
    metadata.resize(padding.max(metadata.len()), b' ');

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("document.json", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(&metadata).unwrap();
    if width != 0 {
        writer
            .start_file("layers/layer-1.png", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&PNG_1X1).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[gpui_kit::test]
fn startup_open_keeps_active_bundle_until_candidate_is_presentable(cx: &mut TestAppContext) {
    let paths = StartupTestFiles::new();
    let missing = paths.0.join("missing.ptz");
    let malformed = paths.file("malformed.ptz");
    let unsupported = paths.file("unsupported.ptz");
    let oversized = paths.file("oversized.ptz");
    let zero = paths.file("zero.ptz");
    let a = paths.file("a.ptz");
    let c = paths.file("c.ptz");
    let io = Arc::new(StartupFileIo::default());
    io.insert(&malformed, b"not a zip archive".to_vec());
    io.insert(
        &unsupported,
        startup_archive("doc-unsupported", 1, 1, true, 0),
    );
    io.insert(
        &oversized,
        startup_archive("doc-oversized", 1, 1, false, 1024 * 1024 + 1),
    );
    io.insert(&zero, startup_archive("doc-zero", 0, 0, false, 0));
    io.insert(&a, startup_archive("doc-a", 1, 1, false, 0));
    io.insert(&c, startup_archive("doc-c", 1, 1, false, 0));
    let (handle, content) = open_app_with(cx, Some((a.clone(), io.clone())));

    cx.update_window(handle.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            assert_eq!(app.active_document_id_for_test().as_deref(), Some("doc-a"));
            assert_eq!(app.active_document_pixel_lengths_for_test(), Some((4, 0)));

            let approvals = io.approvals.load(Ordering::Relaxed);
            let reads = io.reads.load(Ordering::Relaxed);
            assert!(app
                .open_startup_document_for_test(&missing, io.as_ref(), cx)
                .is_err());
            assert_eq!(io.approvals.load(Ordering::Relaxed), approvals);
            assert_eq!(io.reads.load(Ordering::Relaxed), reads);
            assert_eq!(app.active_document_id_for_test().as_deref(), Some("doc-a"));

            for path in [&malformed, &unsupported, &oversized] {
                let result = app.open_startup_document_for_test(path, io.as_ref(), cx);
                assert!(result.is_err(), "accepted {}", path.display());
                assert_eq!(app.active_document_id_for_test().as_deref(), Some("doc-a"));
            }

            assert_eq!(
                app.stage_startup_document_for_test(&c, io.as_ref())
                    .expect("C stages"),
                ("doc-c".to_owned(), 1, 1)
            );
            assert_eq!(app.active_document_id_for_test().as_deref(), Some("doc-a"));

            let approvals = io.approvals.load(Ordering::Relaxed);
            let reads = io.reads.load(Ordering::Relaxed);
            let error = app
                .open_startup_document_for_test(&zero, io.as_ref(), cx)
                .expect_err("0x0 loads but cannot be presented");
            assert!(error.starts_with("document presentation failed:"));
            assert_eq!(io.approvals.load(Ordering::Relaxed), approvals + 1);
            assert_eq!(io.reads.load(Ordering::Relaxed), reads + 1);
            assert_eq!(app.active_document_id_for_test().as_deref(), Some("doc-a"));

            app.open_startup_document_for_test(&c, io.as_ref(), cx)
                .expect("C replaces A");
            assert_eq!(app.active_document_id_for_test().as_deref(), Some("doc-c"));
            assert_eq!(app.active_document_pixel_lengths_for_test(), Some((4, 0)));
        });
    })
    .unwrap();
}

/// The Navigator's no-document body: the dashed no-image state replaces the
/// canvas and its pan hitbox, and the zoom row loses its handlers, so a stray
/// click leaves no error behind.
///
/// RED proof: mount the canvas body regardless of the canvas and the second
/// assert fails on the hitbox the empty branch must not install.
#[gpui_kit::test]
fn navigator_shows_the_no_image_state_without_a_document(cx: &mut TestAppContext) {
    let (handle, content) = open_app(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("navigator-empty").is_some(),
            "the dashed no-image state mounts without a document"
        );
        assert!(
            window.try_find("navigator-hitbox").is_none(),
            "the pan hitbox belongs to the canvas body, not the empty state"
        );
        assert!(
            window.try_find("navigator-zoom-row").is_some(),
            "the zoom row still mounts over the empty state"
        );
        for id in ["navigator-zoom-out", "navigator-zoom-in"] {
            window.click(id, cx);
            window.render_frame(cx);
            assert!(
                content
                    .read(cx)
                    .state_for_test()
                    .read(cx)
                    .status_error
                    .is_none(),
                "{id} must be inert without a document, not a rejected command"
            );
        }
    })
    .unwrap();
}

/// The collapse chevron folds the section to its header and back, and the fit
/// button plus the zoom +/- steps drive the real camera.
///
/// RED proof: wire the buttons to no-ops and the zoom asserts fail on the
/// unchanged camera.
#[gpui_kit::test]
fn navigator_collapse_and_zoom_drive_the_camera(cx: &mut TestAppContext) {
    let paths = StartupTestFiles::new();
    let source = paths.file("nav-zoom.ptz");
    let io = Arc::new(StartupFileIo::default());
    io.insert(&source, startup_archive("doc-nav", 1, 1, false, 0));
    let (handle, content) = open_app_with(cx, Some((source, io)));
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("navigator-empty").is_none(),
            "a document shows the canvas preview, not the empty state"
        );
        assert!(window.try_find("navigator-hitbox").is_some());

        window.click("navigator-fit", cx);
        window.render_frame(cx);
        let fitted = content
            .read(cx)
            .camera_for_test()
            .expect("fit stores a concrete camera")
            .zoom;

        window.click("navigator-zoom-in", cx);
        window.render_frame(cx);
        let zoomed = content
            .read(cx)
            .camera_for_test()
            .expect("camera after zoom-in")
            .zoom;
        assert!(
            zoomed > fitted,
            "the navigator + button must zoom the camera in"
        );

        window.click("navigator-zoom-out", cx);
        window.render_frame(cx);
        let stepped = content
            .read(cx)
            .camera_for_test()
            .expect("camera after zoom-out")
            .zoom;
        assert!(
            stepped < zoomed,
            "the navigator - button must zoom the camera back out"
        );

        window.click("navigator-collapse", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("navigator-hitbox").is_none(),
            "the collapsed section mounts no body"
        );
        window.click("navigator-collapse", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("navigator-hitbox").is_some(),
            "re-expanding mounts the body again"
        );
    })
    .unwrap();
}
