//! Shared drag state for in-app dialogs: grab the titlebar, move the panel
//! within the window frame. Positions are kept for the process lifetime only,
//! matching the session-level feel of desktop editing applications.

use std::cell::RefCell;

use gpui_kit::{point, Point, Size};

use crate::state::DialogId;

const SLOT_COUNT: usize = 10;

thread_local! {
    static DIALOG_OFFSETS: RefCell<[(f32, f32); SLOT_COUNT]> = RefCell::new([(0.0, 0.0); SLOT_COUNT]);
    static DRAG_SESSION: RefCell<Option<DragSession>> = RefCell::new(None);
}

#[derive(Clone, Copy)]
struct DragSession {
    slot: usize,
    grab: Point<gpui_kit::Pixels>,
    origin_offset: (f32, f32),
}

fn slot(dialog: DialogId) -> usize {
    match dialog {
        DialogId::NewDocument => 0,
        DialogId::Settings => 1,
        DialogId::About => 2,
        DialogId::DirtyClose => 3,
        DialogId::Recovery => 4,
        DialogId::Export => 5,
        DialogId::Print => 6,
        DialogId::Rename => 7,
        DialogId::ResizeCanvas => 8,
    }
}

pub(super) fn offset(dialog: DialogId) -> (f32, f32) {
    DIALOG_OFFSETS.with(|offsets| offsets.borrow()[slot(dialog)])
}

/// Per-axis room to shift the centred panel while keeping a grabbable part
/// of it inside the window. Using the shorter viewport edge for both axes
/// starved horizontal drag on wide (maximized) windows.
pub(super) fn offset_limit(viewport: Size<gpui_kit::Pixels>) -> (f32, f32) {
    (
        (f32::from(viewport.width) / 2.0 - 48.0).max(0.0),
        (f32::from(viewport.height) / 2.0 - 48.0).max(0.0),
    )
}

pub(super) fn start_drag(dialog: DialogId, grab: Point<gpui_kit::Pixels>) {
    let index = slot(dialog);
    let origin_offset = DIALOG_OFFSETS.with(|offsets| offsets.borrow()[index]);
    DRAG_SESSION.with(|session| {
        *session.borrow_mut() = Some(DragSession {
            slot: index,
            grab,
            origin_offset,
        })
    });
}

fn clamp(value: f32, limit: f32) -> f32 {
    value.clamp(-limit, limit)
}

pub(super) fn drag_move(
    position: Point<gpui_kit::Pixels>,
    viewport: Size<gpui_kit::Pixels>,
) -> bool {
    let Some(session) = DRAG_SESSION.with(|session| *session.borrow()) else {
        return false;
    };
    let (limit_x, limit_y) = offset_limit(viewport);
    let delta = point(position.x - session.grab.x, position.y - session.grab.y);
    let offset = (
        clamp(session.origin_offset.0 + f32::from(delta.x), limit_x),
        clamp(session.origin_offset.1 + f32::from(delta.y), limit_y),
    );
    let changed = DIALOG_OFFSETS.with(|offsets| {
        let mut offsets = offsets.borrow_mut();
        let changed = offsets[session.slot] != offset;
        offsets[session.slot] = offset;
        changed
    });
    changed
}

pub(super) fn end_drag() -> bool {
    DRAG_SESSION.with(|session| session.borrow_mut().take().is_some())
}
