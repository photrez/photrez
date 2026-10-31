//! The Move tool's empty-canvas layer marquee (`useCanvasMarqueeSelect.ts`).
//!
//! A press that misses every layer opens a session; the drag paints a rect and
//! selects each visible, unlocked, non-background layer whose axis-aligned box
//! overlaps it. Shift/Ctrl keeps the pre-drag selection in the union. A press
//! released below the 3 px threshold without a modifier clears the layer
//! selection, and cancelling restores the pre-drag selection.
//!
//! Native has no null primary row, so "cleared" is the empty `selected_layers`
//! list with row 0 as the inactive primary; the canvas only paints a transform
//! frame for a primary that is part of the selection.

use gpui_kit::Context;

use crate::app::transform_geometry::layer_aabb;
use crate::app::App;

/// The screen-space drag threshold, squared: 3 px (`useCanvasMarqueeSelect.ts`).
const DRAG_THRESHOLD_SQ: f32 = 9.0;

#[derive(Clone, Debug)]
pub(crate) struct MoveMarquee {
    pub start_doc: (f64, f64),
    pub current_doc: (f64, f64),
    pub start_screen: (f32, f32),
    pub additive: bool,
    pub initial_selection: Vec<u64>,
    pub exceeded: bool,
}

impl MoveMarquee {
    /// The marquee rect in document space, `None` below the drag threshold.
    pub(crate) fn rect(&self) -> Option<(f64, f64, f64, f64)> {
        if !self.exceeded {
            return None;
        }
        let min_x = self.start_doc.0.min(self.current_doc.0);
        let min_y = self.start_doc.1.min(self.current_doc.1);
        let max_x = self.start_doc.0.max(self.current_doc.0);
        let max_y = self.start_doc.1.max(self.current_doc.1);
        Some((
            min_x,
            min_y,
            (max_x - min_x).max(1.0),
            (max_y - min_y).max(1.0),
        ))
    }
}

impl App {
    pub(crate) fn begin_move_marquee(
        &mut self,
        doc: (f64, f64),
        screen: (f32, f32),
        additive: bool,
        cx: &Context<Self>,
    ) {
        let initial_selection = self.state.read(cx).selected_layers.clone();
        self.move_marquee = Some(MoveMarquee {
            start_doc: doc,
            current_doc: doc,
            start_screen: screen,
            additive,
            initial_selection,
            exceeded: false,
        });
    }

    pub(crate) fn update_move_marquee(
        &mut self,
        doc: (f64, f64),
        screen: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.move_marquee.as_mut() else {
            return;
        };
        session.current_doc = doc;
        let dx = screen.0 - session.start_screen.0;
        let dy = screen.1 - session.start_screen.1;
        if !session.exceeded && dx * dx + dy * dy > DRAG_THRESHOLD_SQ {
            session.exceeded = true;
        }
        let Some(rect) = session.rect() else {
            return;
        };
        let additive = session.additive;
        let initial = session.initial_selection.clone();
        let (selected, multi_bounds) = self.marquee_selection(rect, additive, &initial, cx);
        self.state.update(cx, |state, _| {
            state.selected_layers = selected;
            state.multi_bounds = multi_bounds;
        });
        cx.notify();
    }

    /// Release handler: a sub-threshold, non-additive press is the oracle's
    /// "click on empty canvas" and deselects every layer row.
    pub(crate) fn finish_move_marquee(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.move_marquee.take() else {
            return;
        };
        if !session.exceeded && !session.additive {
            self.state.update(cx, |state, _| {
                state.selected_layers.clear();
                state.selected_layer = 0;
                state.multi_bounds = None;
            });
            cx.notify();
        }
    }

    /// Cancel handler: a drag that exceeded the threshold put its selection on
    /// screen, so the oracle restores the pre-drag selection.
    pub(crate) fn cancel_move_marquee(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.move_marquee.take() else {
            return;
        };
        if !session.exceeded {
            return;
        }
        let multi_bounds = self.marquee_bounds(&session.initial_selection, cx);
        self.state.update(cx, |state, _| {
            state.selected_layers = session.initial_selection;
            state.multi_bounds = multi_bounds;
        });
        cx.notify();
    }

    /// Layers whose AABB intersects `rect`, skipping hidden, locked and
    /// background layers, unioned with the initial ids when additive.
    fn marquee_selection(
        &self,
        rect: (f64, f64, f64, f64),
        additive: bool,
        initial: &[u64],
        cx: &Context<Self>,
    ) -> (Vec<u64>, Option<(f64, f64, f64, f64)>) {
        let Some(doc_id) = self.active_document.as_ref().map(|doc| doc.doc_id.clone()) else {
            return (initial.to_vec(), None);
        };
        let Some(content) = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
        else {
            return (initial.to_vec(), None);
        };
        let state = self.state.read(cx);
        let (min_x, min_y, width, height) = rect;
        let (max_x, max_y) = (min_x + width, min_y + height);
        let mut selected: Vec<u64> = if additive {
            initial.to_vec()
        } else {
            Vec::new()
        };
        let mut boxes: Vec<(u64, (f64, f64, f64, f64))> = Vec::new();
        for layer in &content.layers {
            if !layer.visible || layer.locked || layer.is_background == Some(true) {
                continue;
            }
            let Some(row) = state
                .layers
                .iter()
                .find(|row| row.core_id.as_deref() == Some(layer.id.as_str()))
            else {
                continue;
            };
            let aabb = layer_aabb(&layer.transform, layer.width, layer.height);
            boxes.push((row.id, aabb));
            let (x, y, w, h) = aabb;
            let overlaps = !(max_x < x || min_x > x + w || max_y < y || min_y > y + h);
            if overlaps && !selected.contains(&row.id) {
                selected.push(row.id);
            }
        }
        let mut bounds: Option<(f64, f64, f64, f64)> = None;
        if selected.len() > 1 {
            for (id, (x, y, w, h)) in boxes {
                if !selected.contains(&id) {
                    continue;
                }
                bounds = Some(match bounds {
                    None => (x, y, w, h),
                    Some((bx, by, bw, bh)) => {
                        let min_x = bx.min(x);
                        let min_y = by.min(y);
                        let max_x = (bx + bw).max(x + w);
                        let max_y = (by + bh).max(y + h);
                        (min_x, min_y, max_x - min_x, max_y - min_y)
                    }
                });
            }
        }
        (selected, bounds)
    }

    /// Union bounds of a row selection, `None` for fewer than two layers.
    fn marquee_bounds(&self, selected: &[u64], cx: &Context<Self>) -> Option<(f64, f64, f64, f64)> {
        if selected.len() < 2 {
            return None;
        }
        let doc_id = self
            .active_document
            .as_ref()
            .map(|doc| doc.doc_id.clone())?;
        let content = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .content_snapshot()?;
        let state = self.state.read(cx);
        let mut bounds: Option<(f64, f64, f64, f64)> = None;
        for row in state.layers.iter().filter(|row| selected.contains(&row.id)) {
            let Some(core_id) = row.core_id.as_deref() else {
                continue;
            };
            let Some(layer) = content.layers.iter().find(|layer| layer.id == core_id) else {
                continue;
            };
            let (x, y, w, h) = layer_aabb(&layer.transform, layer.width, layer.height);
            bounds = Some(match bounds {
                None => (x, y, w, h),
                Some((bx, by, bw, bh)) => {
                    let min_x = bx.min(x);
                    let min_y = by.min(y);
                    let max_x = (bx + bw).max(x + w);
                    let max_y = (by + bh).max(y + h);
                    (min_x, min_y, max_x - min_x, max_y - min_y)
                }
            });
        }
        bounds
    }
}

#[cfg(test)]
impl App {
    /// The live marquee rect, `None` before the drag threshold or at rest.
    pub(crate) fn move_marquee_for_test(&self) -> Option<(f64, f64, f64, f64)> {
        self.move_marquee
            .as_ref()
            .and_then(|marquee| marquee.rect())
    }
}
