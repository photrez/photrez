//! The document tab strip's mounted behaviour: which controls it mounts, and
//! where they sit.

use super::*;

/// The tab strip mounts no layout toggle. This app ships one dock layout, so
/// that control answered no click and it is not mounted here.
///
/// The proof is the measured gap, not an id lookup: `try_find` only sees
/// elements built with `.test_support()`, and the toggle never had one, so an
/// absent-id assertion could not fail. Export keeps its place at the strip's
/// right end and the plus button moves up against it, so only the cluster's
/// 1px border and 8px left padding sit between the two. Any element mounted in
/// the cluster widens that gap, and this test fails.
///
/// RED proof: mount `layout_toggle()` in `views/document_tabs.rs::cluster` and
/// the measured gap grows from 9px to 45px (a 28px button plus the 8px cluster
/// gap on each side of it).
#[gpui_kit::test]
fn the_tab_strip_mounts_no_layout_toggle(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;

    let directory = TestDirectory::new();
    let (window, _content, _source) = mount_fixture_app(cx, &directory);

    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        let plus = window.find("new-document").bounds();
        let export = window.find("export-button").bounds();
        let gap =
            f32::from(export.origin.x) - f32::from(plus.origin.x) - f32::from(plus.size.width);
        assert!(
            (gap - 9.0).abs() < 0.5,
            "nothing is mounted between the plus button and Export, measured {gap}px"
        );
    })
    .expect("test window remains open");
}
