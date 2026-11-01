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
            "S053_RAIL_CLICK_MS samples={samples:?} p50={:.3} p95={:.3}",
            sorted[4], sorted[8]
        );
    })
    .expect("test window remains open");
}
