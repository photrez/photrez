use super::*;
use crate::state::ToolId;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{MouseButton, MouseDownEvent, MouseUpEvent, PlatformInput};

/// The rail mounts 36 px tool buttons with Move active, and clicking a
/// tool switches the active tool. The timed path is the product dispatch plus
/// one render, the same protocol as the menu surfaces.
#[gpui_kit::test]
fn rail_tool_click_switches_and_stays_fluid(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.create_document(200, 150, cx).expect("blank document");
        });
        window.render_frame(cx);
        let move_button = window
            .try_find("move")
            .expect("the Move tool mounts in the rail")
            .bounds();
        assert!(
            (f32::from(move_button.size.width) - 36.0).abs() < 0.5,
            "rail buttons are 36 px, got {}",
            move_button.size.width
        );
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_tool,
            ToolId::Move
        );
        let mut samples = Vec::new();
        for index in 0..9 {
            let id = if index % 2 == 0 { "brush" } else { "move" };
            let position = window
                .try_find(id)
                .expect("the rail tool mounts")
                .bounds()
                .center();
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            let expected = if id == "brush" {
                ToolId::Brush
            } else {
                ToolId::Move
            };
            assert_eq!(
                content.read(cx).state_for_test().read(cx).active_tool,
                expected,
                "clicking {id} selects that tool"
            );
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "RAIL_CLICK_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
    })
    .expect("test window remains open");
}

/// Hovering a rail button publishes the tool into `AppState`, which is what
/// the icon tint resolves from, and leaving clears it. Mounted tests cannot
/// read colour, so this pins the state path only.
///
/// RED proof: remove the `on_hover` listener from `tool_button` and the first
/// hover assertion fails: `hovered_tool` stays `None`.
#[gpui_kit::test]
fn rail_hover_publishes_and_clears_the_hovered_tool(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).hovered_tool,
            None,
            "no tool is hovered at rest"
        );
        window.hover("crop", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).hovered_tool,
            Some(ToolId::Crop),
            "hovering the Crop button publishes it"
        );
        window.hover("canvas-hitbox", cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).hovered_tool,
            None,
            "leaving the button clears the hover"
        );
    })
    .expect("test window remains open");
}

/// A hovered rail button carries its tooltip, the way the reference rail does.
///
/// RED proof: remove the `rail_tooltip` child from `tool_button` and the second
/// assert fails with "hovering the Move button shows its tooltip".
#[gpui_kit::test]
fn rail_hover_shows_the_tool_tooltip(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, _content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("rail-tooltip-move").is_none(),
            "no rail tooltip at rest"
        );
        window.hover("move", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("rail-tooltip-move").is_some(),
            "hovering the Move button shows its tooltip"
        );
        window.hover("canvas-hitbox", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("rail-tooltip-move").is_none(),
            "leaving the button takes the tooltip away"
        );
        window.hover("fg-swatch", cx);
        window.render_frame(cx);
        assert!(
            window.try_find("rail-tooltip-fg").is_some(),
            "hovering the foreground swatch shows its tooltip"
        );
    })
    .expect("test window remains open");
}
