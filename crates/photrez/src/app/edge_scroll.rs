//! Edge auto-scroll: while a drag is live and the pointer sits within 40 px of
//! a canvas edge, pan the camera with a smoothstep ramp at 45% of the axis per
//! second. Screen-space speed, so it is zoom-independent.

use std::time::{Duration, Instant};

use gpui_kit::{Context, Pixels, Point};

use crate::state::ToolId;

use super::App;

const EDGE_ZONE: f32 = 40.;
const SPEED_FACTOR: f32 = 0.45;
const TICK: Duration = Duration::from_millis(16);
/// A stalled loop or a suspended process must not turn into a teleport when
/// it resumes. Three ticks is the most one frame may contribute.
const MAX_TICK_DT: f32 = 0.05;

fn smoothstep(t: f32) -> f32 {
    let c = t.clamp(0., 1.);
    c * c * (3. - 2. * c)
}

impl App {
    /// Called on every move while a drag is live: refresh the edge cursor and
    /// start the loop when the pointer enters a zone.
    pub(crate) fn track_edge_scroll(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.edge_pointer = Some(position);
        if self.edge_scroll_active {
            return;
        }
        let generating_tool = {
            let state = self.state.read(cx);
            matches!(
                state.active_tool,
                ToolId::Brush | ToolId::Eraser | ToolId::Move | ToolId::Selection | ToolId::Crop
            )
        };
        let dragging =
            generating_tool && (self.input.has_active_brush() || self.pending_gesture.is_some());
        if !dragging || !self.in_edge_zone(position) {
            return;
        }
        self.start_edge_scroll(cx);
    }

    pub(crate) fn stop_edge_scroll(&mut self) {
        self.edge_scroll_active = false;
        self.edge_scroll_generation = self.edge_scroll_generation.wrapping_add(1);
        self.edge_pointer = None;
    }

    fn in_edge_zone(&self, position: Point<Pixels>) -> bool {
        let Some(bounds) = self.canvas_bounds.get() else {
            return false;
        };
        let local_x = f32::from(position.x) - f32::from(bounds.origin.x);
        let local_y = f32::from(position.y) - f32::from(bounds.origin.y);
        let width: f32 = bounds.size.width.into();
        let height: f32 = bounds.size.height.into();
        let dist_x = if local_x < width / 2. {
            local_x
        } else {
            width - local_x
        };
        let dist_y = if local_y < height / 2. {
            local_y
        } else {
            height - local_y
        };
        dist_x < EDGE_ZONE || dist_y < EDGE_ZONE
    }

    fn start_edge_scroll(&mut self, cx: &mut Context<Self>) {
        self.edge_scroll_active = true;
        self.edge_scroll_generation = self.edge_scroll_generation.wrapping_add(1);
        let generation = self.edge_scroll_generation;
        let task = cx.spawn(async move |weak, cx| {
            let mut last = Instant::now();
            loop {
                cx.background_executor().timer(TICK).await;
                let now = Instant::now();
                let dt = (now - last).as_secs_f32();
                last = now;
                let keep_going = weak
                    .update(cx, |app, cx| app.edge_scroll_tick(generation, dt, cx))
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        });
        self.edge_scroll_task = Some(task);
    }

    #[cfg(test)]
    pub(crate) fn edge_scroll_active_for_test(&self) -> bool {
        self.edge_scroll_active
    }

    #[cfg(test)]
    pub(crate) fn edge_scroll_step_for_test(&mut self, dt: f32, cx: &mut Context<Self>) -> bool {
        let generation = self.edge_scroll_generation;
        self.edge_scroll_tick(generation, dt, cx)
    }

    /// One pan tick. Returns `false` when the pointer left every zone or the
    /// drag ended, which stops the loop.
    pub(crate) fn edge_scroll_tick(
        &mut self,
        generation: u64,
        dt: f32,
        cx: &mut Context<Self>,
    ) -> bool {
        if generation != self.edge_scroll_generation || !self.edge_scroll_active {
            return false;
        }
        let dt = dt.clamp(0., MAX_TICK_DT);
        let Some(position) = self.edge_pointer else {
            return false;
        };
        let dragging = self.input.has_active_brush() || self.pending_gesture.is_some();
        let Some(bounds) = self.canvas_bounds.get() else {
            return false;
        };
        if !dragging {
            return false;
        }
        let local_x = f32::from(position.x) - f32::from(bounds.origin.x);
        let local_y = f32::from(position.y) - f32::from(bounds.origin.y);
        let width: f32 = bounds.size.width.into();
        let height: f32 = bounds.size.height.into();
        let dir_x = if local_x < width / 2. { 1. } else { -1. };
        let dir_y = if local_y < height / 2. { 1. } else { -1. };
        let dist_x = if local_x < width / 2. {
            local_x
        } else {
            width - local_x
        };
        let dist_y = if local_y < height / 2. {
            local_y
        } else {
            height - local_y
        };
        if dist_x >= EDGE_ZONE && dist_y >= EDGE_ZONE {
            return false;
        }
        let t_x = smoothstep((EDGE_ZONE - dist_x) / EDGE_ZONE);
        let t_y = smoothstep((EDGE_ZONE - dist_y) / EDGE_ZONE);
        let pan_x = dir_x * t_x * width * SPEED_FACTOR * dt;
        let pan_y = dir_y * t_y * height * SPEED_FACTOR * dt;
        if pan_x == 0. && pan_y == 0. {
            return true;
        }
        let Some(mut camera) = self.effective_camera() else {
            return false;
        };
        camera.pan(pan_x, pan_y);
        self.set_camera(camera, cx);
        true
    }
}
