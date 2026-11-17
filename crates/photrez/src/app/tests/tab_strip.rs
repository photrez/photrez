//! The document tab strip's mounted behaviour: which controls it mounts, and
//! where they sit.

use super::*;

/// The tab strip mounts the dock layout toggle between the plus button and
/// Export. The toggle flips `AppState::right_dock_layout`; the accent tint
/// in the stacked state is painted from the same field.
///
/// The proof is the measured gap, not an id lookup alone: with the toggle
/// mounted between the two, the measured gap is 45px (8px cluster left
/// padding + 28px button + 8px cluster gap plus a 1px border, per the
/// `+` → `export` render). Clicking the toggle twice returns the dock to
/// its original arrangement, so this is also the mounted analogue of the
/// runtime click-through.
///
/// History: this test asserted the *absence* of the toggle (the app shipped
/// one dock layout then, so the control was removed); the owner re-approved
/// mounting it with a handler, and the assertion flipped accordingly.
#[gpui_kit::test]
fn the_tab_strip_mounts_the_dock_layout_toggle_and_it_flips_the_dock(
    cx: &mut gpui_kit::TestAppContext,
) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let initial = content.read(cx).state_for_test().read(cx).right_dock_layout;
        let plus = window.find("new-document").bounds();
        let toggle = window.find("tab-dock-layout").bounds();
        let export = window.find("export-button").bounds();
        let gap =
            f32::from(export.origin.x) - f32::from(plus.origin.x) - f32::from(plus.size.width);
        assert!(
            (gap - 45.0).abs() < 0.5,
            "the dock layout toggle sits between the plus button and Export, measured {gap}px"
        );
        assert!(
            f32::from(toggle.origin.x) >= f32::from(plus.origin.x) + f32::from(plus.size.width),
            "the toggle is not left of the plus button"
        );
        assert!(
            f32::from(toggle.origin.x) + f32::from(toggle.size.width) <= f32::from(export.origin.x),
            "the toggle is not right of the export button"
        );

        window.click("tab-dock-layout", cx);
        let after_click = content.read(cx).state_for_test().read(cx).right_dock_layout;
        assert_ne!(
            initial, after_click,
            "clicking the toggle must flip right_dock_layout"
        );

        window.click("tab-dock-layout", cx);
        let restored = content.read(cx).state_for_test().read(cx).right_dock_layout;
        assert_eq!(
            initial, restored,
            "clicking the toggle again must restore the original layout"
        );
    })
    .expect("test window remains open");
}
