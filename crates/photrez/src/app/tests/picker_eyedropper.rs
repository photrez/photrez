//! Sampling while the picker is open goes to the canvas handler and to no code of
//! the picker's own: the panel's click-outside handler closes the picker session
//! first, so the dismissal's revert lands before the canvas handler writes the
//! sampled colour into the foreground slot. The sample never writes a pixel. These
//! tests hold that ordering, the slot the sample lands in, and both immovable
//! states (the document and its history).
//!
//! These tests fail when that behaviour changes. Proven by temporarily: dropping the
//! panel's click-outside close, which leaves every test failing on its "the sample
//! dismisses the picker" line because the session is still open; writing the sample
//! into the background slot instead of the foreground one, which fails all three on
//! the slot claims; deleting the early return in the brush's held-alternate branch,
//! so the pick also starts a stroke and the history version moves from 0 to 1; and
//! making every sample write the document, which fails all three on the
//! "sampling writes no pixels" line with the composited pixels reading
//! `[[7, 7, 7, 255], [255, 0, 0, 255]]` instead of `[[0, 0, 255, 255], [255, 0, 0,
//! 255]]`.
//!
//! A sampled colour is not proof of a read-only sample: the obvious probe - filling
//! the document with the foreground after sampling - repaints the colour it just
//! read, so the document looks untouched. The last mutation above therefore writes a
//! different colour and restores the slot afterwards.

use super::input::{drag_doc, mount_archive_app, page, set_tool, window_point};
use super::*;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{px, MouseButton, MouseDownEvent, MouseUpEvent, PlatformInput, Point};

/// The two-layer project is two document pixels wide and one tall.
const DOCUMENT: (u32, u32) = (2, 1);

/// The composite colour at document pixel `(0, 0)` of the two-layer project:
/// blue on top, red on the selected bottom layer.
const SAMPLED_AT_ZERO: [u8; 4] = [0, 0, 255, 255];

fn history_version(content: &Entity<App>, cx: &gpui_kit::App) -> u64 {
    content
        .read(cx)
        .active_core_history_version_for_test()
        .expect("the project has a history version")
}

/// The two paint slots and whether a picker session is open.
fn slots(state: &crate::state::AppState) -> ([u8; 4], [u8; 4], bool) {
    (state.fg_color, state.bg_color, state.color_picker.is_some())
}

fn live_slots(content: &Entity<App>, cx: &gpui_kit::App) -> ([u8; 4], [u8; 4], bool) {
    slots(content.read(cx).state_for_test().read(cx))
}

/// The document as it is composited for the canvas, pixel by pixel. A stroke or a
/// fill on any layer shows up here, so this is what sampling must leave alone.
fn composited(content: &Entity<App>, cx: &gpui_kit::App) -> Vec<[u8; 4]> {
    let reader = content.read(cx);
    let (width, height) = reader
        .active_document_dims_for_test()
        .expect("the project has an active document");
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .map(|(x, y)| {
            content
                .read(cx)
                .active_composite_pixel_for_test(x, y)
                .expect("the document pixel is inside the composite")
        })
        .collect()
}

/// A sample taken after the picker edited the colour: the dismissal reverts the
/// edit, the sample still applies, the background slot and the document are
/// untouched, and no history entry appears.
#[gpui_kit::test]
fn a_sample_after_a_picker_edit_keeps_the_sampled_colour(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );

    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Eyedropper, 1.0);
        window.render_frame(cx);
        let composite_before = composited(&content, cx);
        let history_before = history_version(&content, cx);
        let (_fg_before, bg_before, _) = live_slots(&content, cx);

        window.click("fg-swatch", cx);
        window.render_frame(cx);
        window.click_at(
            "color-picker-sv",
            Point {
                x: px(160.0),
                y: px(20.0),
            },
            cx,
        );
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let (fg, _, open) = slots(app.state_for_test().read(cx));
            assert!(open, "the surface edit leaves the picker open");
            assert_ne!(fg, [0, 0, 0, 255], "the surface edit is live");
        });

        // The sample point sits left of the panel, as any canvas click does.
        drag_doc(window, DOCUMENT, (0.5, 0.5), (0.5, 0.5), cx);
        content.update(cx, |app, cx| {
            let (fg, bg, open) = slots(app.state_for_test().read(cx));
            assert!(!open, "the sample dismisses the picker");
            assert_eq!(
                fg, SAMPLED_AT_ZERO,
                "the sample survives the dismissal revert instead of being undone by it"
            );
            assert_eq!(bg, bg_before, "sampling leaves the other slot alone");
        });
        assert_eq!(
            composited(&content, cx),
            composite_before,
            "sampling writes no pixels"
        );
        assert_eq!(
            history_version(&content, cx),
            history_before,
            "sampling adds no history entry"
        );
    })
    .expect("test window remains open");
}

/// The same sample with the background picker open: the background slot keeps
/// its own colour and the sample still lands in the foreground, because the
/// sampler has one destination.
#[gpui_kit::test]
fn a_sample_with_the_background_picker_open_samples_the_foreground(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );

    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Eyedropper, 1.0);
        window.render_frame(cx);
        let composite_before = composited(&content, cx);
        let history_before = history_version(&content, cx);
        let (fg_before, bg_before, _) = live_slots(&content, cx);
        assert_eq!(
            fg_before,
            [0, 0, 0, 255],
            "the sampler opens on the black slot"
        );

        // The background swatch sits under the foreground one; its exposed
        // corner is what a pointer can reach.
        window.click_at(
            "bg-swatch",
            Point {
                x: px(20.0),
                y: px(20.0),
            },
            cx,
        );
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let state = app.state_for_test().read(cx);
            assert_eq!(
                state.color_picker.as_ref().expect("picker opens").target,
                crate::state::ColorTarget::Background,
                "the background swatch owns this session"
            );
        });

        window.click_at(
            "color-picker-sv",
            Point {
                x: px(160.0),
                y: px(20.0),
            },
            cx,
        );
        window.render_frame(cx);
        content.update(cx, |app, cx| {
            let (fg, bg, _) = slots(app.state_for_test().read(cx));
            assert_ne!(bg, bg_before, "the box edits the background live");
            assert_eq!(
                fg, fg_before,
                "the background session ignores the foreground"
            );
        });

        drag_doc(window, DOCUMENT, (0.5, 0.5), (0.5, 0.5), cx);
        content.update(cx, |app, cx| {
            let (fg, bg, open) = slots(app.state_for_test().read(cx));
            assert!(!open, "the sample dismisses the background picker");
            assert_eq!(bg, bg_before, "the background keeps its own colour");
            assert_eq!(
                fg, SAMPLED_AT_ZERO,
                "the sample lands in the foreground slot it always writes"
            );
        });
        assert_eq!(
            composited(&content, cx),
            composite_before,
            "sampling writes no pixels"
        );
        assert_eq!(
            history_version(&content, cx),
            history_before,
            "sampling adds no history entry"
        );
    })
    .expect("test window remains open");
}

/// The brush's held-alternate sample is the other way to take a colour with the
/// picker open: it dismisses the picker, samples, and starts no stroke.
#[gpui_kit::test]
fn an_alternate_sample_with_the_brush_while_the_picker_is_open_paints_nothing(
    cx: &mut gpui_kit::TestAppContext,
) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_archive_app(
        cx,
        &directory,
        super::commands::multi_layer_archive_with(false),
    );

    cx.update_window(window.into(), |_, window, cx| {
        set_tool(&content, cx, ToolId::Brush, 1.0);
        window.render_frame(cx);
        // The top layer of that project is one pixel, so this is the layer a
        // stray stroke would have painted.
        let composite_before = composited(&content, cx);
        let history_before = history_version(&content, cx);
        let (_fg_before, bg_before, _) = live_slots(&content, cx);

        window.click("fg-swatch", cx);
        window.render_frame(cx);
        assert!(
            window.find("color-picker").visible(),
            "the picker is open before the sample"
        );

        let (bounds, placement) = page(window, DOCUMENT);
        let at = window_point(bounds, placement, DOCUMENT, 0.5, 0.5);
        let modifiers = gpui_kit::Modifiers {
            alt: true,
            ..Default::default()
        };
        let _ = window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: at,
                modifiers,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);

        content.update(cx, |app, cx| {
            let (fg, bg, open) = slots(app.state_for_test().read(cx));
            assert!(!open, "the sample dismisses the picker");
            assert_eq!(
                fg, SAMPLED_AT_ZERO,
                "the alternate sample reaches the foreground slot"
            );
            assert_eq!(bg, bg_before, "sampling leaves the other slot alone");
        });
        assert_eq!(
            composited(&content, cx),
            composite_before,
            "an alternate sample starts no stroke"
        );
        assert_eq!(
            history_version(&content, cx),
            history_before,
            "an alternate sample adds no history entry"
        );
    })
    .expect("test window remains open");
}
