//! Mounted contracts for the paint-colour picker: the tool-rail swatches open
//! it, the SV/hue surfaces paint live, Escape/Cancel revert, Enter/OK commit,
//! the hex field replaces a complete value, and swap exchanges the pair.
//!
//! RED proofs (recorded in the 78 ledger):
//! * dropping the `!commit` revert in `close_color_picker` fails
//!   `escape_and_cancel_restore_the_opening_color`;
//! * mapping `sv` to `(x, y)` instead of `(x, 1 - y)` fails
//!   `fg_swatch_opens_the_picker_and_the_sv_box_paints_live`;
//! * giving `ColorTarget::Text` the foreground branch fails
//!   `text_swatch_opens_the_picker_and_edits_the_selected_layer`.

use super::input::{drag_doc, pixels, set_tool};
use super::*;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{px, Point};

const OPENING_FG: [u8; 4] = [225, 90, 23, 255];
const OPENING_BG: [u8; 4] = [255, 255, 255, 255];

/// The colour a click at (160, 20) inside the 200x200 SV box must produce:
/// `s = 0.8`, `v = 1 - 0.1 = 0.9`, hue unchanged from the opening colour.
fn sv_click_color(opening: [u8; 4]) -> [u8; 4] {
    let h = crate::palette::rgb_to_hsv(opening).h;
    crate::palette::hsv_to_rgb(crate::palette::Hsv { h, s: 0.8, v: 0.9 })
}

fn read_colors(app: &App, cx: &gpui_kit::App) -> ([u8; 4], [u8; 4], bool) {
    let state = app.state_for_test().read(cx);
    (state.fg_color, state.bg_color, state.color_picker.is_some())
}

fn click_sv_and_check(
    cx: &mut gpui_kit::App,
    window: &mut gpui_kit::Window,
    content: &Entity<App>,
) {
    window.click_at(
        "color-picker-sv",
        Point {
            x: px(160.),
            y: px(20.),
        },
        cx,
    );
    window.render_frame(cx);
    content.update(cx, |app, cx| {
        let (fg, _, open) = read_colors(app, cx);
        assert!(open, "the picker stays open after a surface click");
        assert_eq!(
            fg,
            sv_click_color(OPENING_FG),
            "the SV box paints the fg colour live"
        );
    });
}

/// The rail swatch opens the picker and the SV surface edits the live colour.
#[gpui_kit::test]
fn fg_swatch_opens_the_picker_and_the_sv_box_paints_live(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        window.click("fg-swatch", cx);
        window.render_frame(cx);
        assert!(
            window.find("color-picker").visible(),
            "the foreground swatch opens the picker"
        );
        assert!(
            window.find("color-picker-sv").visible()
                && window.find("color-picker-hue").visible()
                && window.find("color-picker-hex").visible(),
            "the SV box, hue strip and hex field are mounted"
        );

        click_sv_and_check(cx, window, &content);
    })
    .expect("test window remains open");
}

/// Escape and the Cancel button both restore the colour the picker opened on.
#[gpui_kit::test]
fn escape_and_cancel_restore_the_opening_color(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        window.click("fg-swatch", cx);
        window.render_frame(cx);
        click_sv_and_check(cx, window, &content);

        window.press("escape", cx);
        assert!(
            window.try_find("color-picker").is_none(),
            "escape closes the picker"
        );
        content.update(cx, |app, cx| {
            let (fg, _, open) = read_colors(app, cx);
            assert!(!open);
            assert_eq!(fg, OPENING_FG, "escape restores the opening colour");
        });

        window.click("fg-swatch", cx);
        window.render_frame(cx);
        click_sv_and_check(cx, window, &content);

        window.click("color-picker-cancel", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("color-picker").is_none(),
            "cancel closes the picker"
        );
        content.update(cx, |app, cx| {
            let (fg, _, open) = read_colors(app, cx);
            assert!(!open);
            assert_eq!(fg, OPENING_FG, "cancel restores the opening colour");
        });
    })
    .expect("test window remains open");
}

/// Enter and OK keep the edited colour and close the picker.
#[gpui_kit::test]
fn enter_and_ok_commit_the_edited_color(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        window.click("fg-swatch", cx);
        window.render_frame(cx);
        click_sv_and_check(cx, window, &content);
        window.press("enter", cx);
        assert!(
            window.try_find("color-picker").is_none(),
            "enter closes the picker"
        );
        content.update(cx, |app, cx| {
            let (fg, _, open) = read_colors(app, cx);
            assert!(!open);
            assert_eq!(
                fg,
                sv_click_color(OPENING_FG),
                "enter keeps the edited colour"
            );
        });

        window.click("fg-swatch", cx);
        window.render_frame(cx);
        click_sv_and_check(cx, window, &content);
        window.click("color-picker-ok", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("color-picker").is_none(),
            "ok closes the picker"
        );
        content.update(cx, |app, cx| {
            let (fg, _, open) = read_colors(app, cx);
            assert!(!open);
            assert_eq!(fg, sv_click_color(OPENING_FG), "ok keeps the edited colour");
        });
    })
    .expect("test window remains open");
}

/// Typing a hex value replaces the complete value the field opens with.
#[gpui_kit::test]
fn hex_entry_replaces_the_complete_value_and_commits(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        window.click("fg-swatch", cx);
        window.render_frame(cx);

        window.input("00FF00", cx);
        window.press("enter", cx);
        assert!(
            window.try_find("color-picker").is_none(),
            "a parsed hex value commits on enter"
        );
        content.update(cx, |app, cx| {
            let (fg, _, open) = read_colors(app, cx);
            assert!(!open);
            assert_eq!(
                fg,
                [0, 255, 0, 255],
                "the typed hex becomes the paint colour"
            );
        });
    })
    .expect("test window remains open");
}

/// An unparsable hex value keeps the picker open instead of committing junk.
#[gpui_kit::test]
fn incomplete_hex_keeps_the_picker_open(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        window.click("fg-swatch", cx);
        window.render_frame(cx);

        window.input("ff", cx);
        window.press("enter", cx);
        assert!(
            window.find("color-picker").visible(),
            "an incomplete hex value keeps the picker open"
        );
        content.update(cx, |app, cx| {
            let (fg, _, open) = read_colors(app, cx);
            assert!(open);
            assert_eq!(
                fg, OPENING_FG,
                "an unparsed hex value never changes the colour"
            );
        });
    })
    .expect("test window remains open");
}

/// The swap control exchanges the foreground and background swatches.
#[gpui_kit::test]
fn swap_button_exchanges_the_pair(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        // The swap control is the rail's hover micro-button, hidden at rest.
        window.hover("fg-swatch", cx);
        window.render_frame(cx);
        assert!(
            window.find("swap-colors").visible(),
            "the swap control appears on swatch hover"
        );
        window.click("swap-colors", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let (fg, bg, _) = read_colors(app, cx);
            assert_eq!(fg, OPENING_BG);
            assert_eq!(bg, OPENING_FG);
        });
    })
    .expect("test window remains open");
}

/// The background swatch edits only the background colour.
#[gpui_kit::test]
fn bg_swatch_edits_only_the_background(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        // The background swatch sits under the foreground one; its exposed
        // corner is what a pointer can actually reach.
        window.click_at(
            "bg-swatch",
            Point {
                x: px(20.),
                y: px(20.),
            },
            cx,
        );
        window.render_frame(cx);
        assert!(
            window.find("color-picker").visible(),
            "the background swatch opens the picker"
        );

        window.click_at(
            "color-picker-sv",
            Point {
                x: px(160.),
                y: px(20.),
            },
            cx,
        );
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let (fg, bg, open) = read_colors(app, cx);
            assert!(open);
            assert_eq!(
                fg, OPENING_FG,
                "the background swatch leaves the fg colour alone"
            );
            assert_eq!(
                bg,
                sv_click_color(OPENING_BG),
                "the SV box paints the bg colour"
            );
        });

        window.click("color-picker-cancel", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let (fg, bg, _) = read_colors(app, cx);
            assert_eq!(fg, OPENING_FG);
            assert_eq!(bg, OPENING_BG, "cancel restores the opening background");
        });
    })
    .expect("test window remains open");
}

/// Places and commits a text layer through the production text tool, leaving
/// it selected (edit mode).
fn create_text_layer(window: &mut gpui_kit::Window, content: &Entity<App>, cx: &mut gpui_kit::App) {
    let (w, h, _) = pixels(content, cx);
    drag_doc(window, (w, h), (0.5, 0.5), (0.5, 0.5), cx);
    window.press("a", cx);
    content.update(cx, |app, cx| {
        let _ = app.commit_text_edit(cx);
    });
    window.render_frame(cx);
}

/// With a text layer selected the swatch shows and edits that layer's colour,
/// and Cancel restores it. The paint foreground is untouched.
#[gpui_kit::test]
fn text_swatch_opens_the_picker_and_edits_the_selected_layer(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        create_text_layer(window, &content, cx);

        content.update(cx, |app, cx| {
            let state = app.state_for_test().read(cx);
            assert_eq!(
                state.selected_layer_type.as_deref(),
                Some("text"),
                "the committed text layer is the selection"
            );
            assert_eq!(
                state.selected_text_color,
                Some([0, 0, 0, 255]),
                "edit mode projects the selected layer's colour"
            );
        });

        window.click("opt-text-color", cx);
        window.render_frame(cx);
        assert!(
            window.find("color-picker").visible(),
            "the text swatch opens the picker"
        );

        click_sv_and_check_text_layer(window, &content, cx);

        window.click("color-picker-cancel", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let data = app.text_layer_data_for_test().expect("text data exists");
            assert_eq!(data.color, "#000000", "cancel restores the layer colour");
            let (fg, _, _) = read_colors(app, cx);
            assert_eq!(fg, [0, 0, 0, 255], "cancel leaves the paint colour alone");
        });
    })
    .expect("test window remains open");
}

fn click_sv_and_check_text_layer(
    window: &mut gpui_kit::Window,
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
) {
    window.click_at(
        "color-picker-sv",
        Point {
            x: px(160.),
            y: px(20.),
        },
        cx,
    );
    window.render_frame(cx);
    let picked = crate::palette::to_hex(sv_click_color([0, 0, 0, 255]));
    content.update(cx, |app, cx| {
        assert!(read_colors(app, cx).2, "the picker stays open");
        let data = app.text_layer_data_for_test().expect("text data exists");
        assert_eq!(data.color, picked, "the picker edits the layer colour");
        assert_eq!(
            read_colors(app, cx).0,
            [0, 0, 0, 255],
            "edit mode leaves the paint colour alone"
        );
    });
}

/// Without a text layer the same swatch follows the capture: it shows the
/// paint foreground, carries its tooltip, and paints that colour.
#[gpui_kit::test]
fn text_swatch_paints_the_foreground_in_draw_mode(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Text, 1.0);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert!(
                app.state_for_test().read(cx).selected_text_color.is_none(),
                "draw mode has no text-layer colour"
            );
        });

        window.hover("opt-text-color", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("opt-text-color-tooltip").is_some(),
            "the text swatch carries its text-color tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);

        window.click("opt-text-color", cx);
        window.render_frame(cx);
        assert!(
            window.find("color-picker").visible(),
            "draw mode opens the same picker"
        );
        window.click_at(
            "color-picker-sv",
            Point {
                x: px(160.),
                y: px(20.),
            },
            cx,
        );
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert_eq!(
                read_colors(app, cx).0,
                sv_click_color([0, 0, 0, 255]),
                "draw mode paints the foreground"
            );
            assert!(
                app.text_layer_data_for_test().is_none(),
                "draw mode creates no text layer"
            );
        });

        window.click("color-picker-cancel", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            assert_eq!(
                read_colors(app, cx).0,
                [0, 0, 0, 255],
                "cancel restores the opening foreground"
            );
        });
    })
    .expect("test window remains open");
}

/// A click outside the panel dismisses the picker and reverts, like Cancel.
#[gpui_kit::test]
fn outside_click_dismisses_and_reverts(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        window.click("fg-swatch", cx);
        window.render_frame(cx);
        click_sv_and_check(cx, window, &content);

        window.click_at(
            "color-picker-layer",
            Point {
                x: px(40.),
                y: px(700.),
            },
            cx,
        );
        window.render_frame(cx);
        assert!(
            window.try_find("color-picker").is_none(),
            "a click outside the panel dismisses the picker"
        );
        content.update(cx, |app, cx| {
            let (fg, _, open) = read_colors(app, cx);
            assert!(!open);
            assert_eq!(
                fg, OPENING_FG,
                "an outside click restores the opening colour"
            );
        });
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn shape_fill_swatch_opens_the_picker_and_edits_the_selected_shape(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
        });
        set_tool(&content, cx, ToolId::Shape, 1.0);
        window.render_frame(cx);
        drag_doc(window, (200, 150), (10.0, 10.0), (80.0, 80.0), cx);
        window.render_frame(cx);
        window.click("opt-fill-color", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let state = app.state_for_test();
            let session = state.read(cx).color_picker.clone().expect("picker opens");
            assert_eq!(session.target, crate::state::ColorTarget::ShapeFill);
        });
        window.input("00FF00", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let state = app.state_for_test();
            assert_eq!(
                state.read(cx).fg_color,
                [0, 255, 0, 255],
                "setFillColor always follows the paint foreground"
            );
        });
        let fill = {
            let app = content.read(cx);
            let doc_id = app
                .active_document
                .as_ref()
                .expect("active document")
                .doc_id
                .clone();
            let snapshot = app
                .sessions
                .get(&doc_id)
                .expect("active session")
                .loaded_document()
                .content_snapshot()
                .expect("canonical content");
            snapshot
                .layers
                .iter()
                .find(|layer| layer.name == "Shape")
                .and_then(|layer| layer.shape_params.as_ref())
                .map(|params| params.fill.color.clone())
                .unwrap_or_else(|| {
                    panic!(
                        "the drawn shape carries params: {:?}",
                        snapshot
                            .layers
                            .iter()
                            .map(|layer| (layer.name.clone(), layer.layer_type.clone()))
                            .collect::<Vec<_>>()
                    )
                })
        };
        assert_eq!(
            fill, "#00FF00",
            "the selected shape's fill colour follows the picker"
        );
    })
    .expect("test window remains open");
}
