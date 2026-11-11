//! The animated selection marquee: complementary black and white dashes.

use gpui_kit::AnyElement;

use super::*;

/// Paint one closed ring of marquee dashes. `offset` is the animated phase in
/// logical pixels; the white layer fills the black layer's gaps, so one of
/// the two stays visible on any page content.
pub(super) fn painted_ants(
    window: &mut gpui_kit::Window,
    ring: &[(f32, f32)],
    hairline: f32,
    offset: f32,
) {
    painted_dashed_polyline(
        window,
        ring,
        hairline,
        4.0,
        4.0,
        offset,
        hsla(0., 0., 0., 1.),
    );
    painted_dashed_polyline(
        window,
        ring,
        hairline,
        4.0,
        4.0,
        offset + 4.0,
        hsla(0., 0., 1., 1.),
    );
}

pub(super) fn painted_ants_ellipse(
    window: &mut gpui_kit::Window,
    center: (f32, f32),
    radius_x: f32,
    radius_y: f32,
    hairline: f32,
    offset: f32,
) {
    painted_dashed_ellipse(
        window,
        center,
        radius_x,
        radius_y,
        hairline,
        4.0,
        4.0,
        offset,
        hsla(0., 0., 0., 1.),
    );
    painted_dashed_ellipse(
        window,
        center,
        radius_x,
        radius_y,
        hairline,
        4.0,
        4.0,
        offset + 4.0,
        hsla(0., 0., 1., 1.),
    );
}

/// Wrap the canvas in a repeating animation on the shared clock. The animator
/// runs before paint and publishes its phase to the paint closure.
pub(super) fn animated_canvas(
    element: impl IntoElement + 'static,
    phase: Rc<Cell<f32>>,
    animate: bool,
) -> AnyElement {
    if !animate {
        return element.into_any_element();
    }
    element
        .with_animation(
            "canvas-marching-ants",
            gpui_kit::Animation::new(std::time::Duration::from_millis(500))
                .repeat_synced()
                .with_max_fps(30.0),
            move |element, value| {
                phase.set(value);
                element
            },
        )
        .into_any_element()
}
