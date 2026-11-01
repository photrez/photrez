//! Undo/redo over the document's unified history stream.
//!
//! The core cursor is shared by pixel and metadata entries. A pixel step hands
//! back the exact restored tiles, which land through the display seam without a
//! full composite; a metadata step has no tile handoff, so the page is
//! recomposited.

use std::sync::Arc;

use gpui_kit::{Context, RenderImage};
use photrez_core::protocol::{Command, CommandEnvelope, PixelPatchHandoff, CONTRACT_VERSION};

use super::App;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct UndoDocument;

#[derive(Clone, PartialEq, Debug, Default, gpui_kit::Action)]
#[action(namespace = photrez, no_json)]
pub(crate) struct RedoDocument;

impl App {
    pub(crate) fn undo_document_action(&mut self, _: &UndoDocument, cx: &mut Context<Self>) {
        if self.crop_mini_undos_active(cx) {
            if self.crop_undo(cx) {
                self.set_status_error(None, cx);
            }
            return;
        }
        self.history_action(true, cx);
    }

    pub(crate) fn redo_document_action(&mut self, _: &RedoDocument, cx: &mut Context<Self>) {
        if self.crop_mini_undos_active(cx) {
            if self.crop_redo(cx) {
                self.set_status_error(None, cx);
            }
            return;
        }
        self.history_action(false, cx);
    }

    fn crop_mini_undos_active(&self, cx: &Context<Self>) -> bool {
        let state = self.state.read(cx);
        state.active_tool == crate::state::ToolId::Crop && state.crop_rect.is_some()
    }

    /// Undo enablement: a live
    /// transform session, the crop tool's mini undo, or core history.
    pub(crate) fn can_undo_document(&self, cx: &Context<Self>) -> bool {
        let state = self.state.read(cx);
        if self.transform_drag.is_some() || self.group_transform_drag.is_some() {
            return true;
        }
        if state.active_tool == crate::state::ToolId::Crop && !state.crop_history.is_empty() {
            return true;
        }
        self.history_cursor().is_some_and(|(cursor, _)| cursor > 0)
    }

    /// `edit.redo` enablement, mirroring [`Self::can_undo_document`].
    pub(crate) fn can_redo_document(&self, cx: &Context<Self>) -> bool {
        let state = self.state.read(cx);
        if self.transform_drag.is_some() || self.group_transform_drag.is_some() {
            return true;
        }
        if state.active_tool == crate::state::ToolId::Crop && !state.crop_redo.is_empty() {
            return true;
        }
        self.history_cursor()
            .is_some_and(|(cursor, entries)| cursor < entries)
    }

    fn history_cursor(&self) -> Option<(usize, usize)> {
        let doc_id = self.active_document.as_ref()?.doc_id.clone();
        let query = self
            .sessions
            .get(&doc_id)?
            .loaded_document()
            .registry
            .history_query(&doc_id)?;
        Some((query.cursor as usize, query.entries.len()))
    }

    fn history_action(&mut self, undo: bool, cx: &mut Context<Self>) {
        match self.step_history(undo, cx) {
            Ok(true) => self.set_status_error(None, cx),
            Ok(false) => {}
            Err(error) => self.set_status_error(Some(error), cx),
        }
        cx.notify();
    }

    /// Project the active document's unified history stream into the panel.
    ///
    /// Row 0 is the initial "Open" state (cursor 0); entry `i` is row `i + 1`.
    pub(crate) fn project_history(&mut self, cx: &mut Context<Self>) {
        let rows = self
            .active_document
            .as_ref()
            .and_then(|document| {
                let query = self
                    .sessions
                    .get(&document.doc_id)?
                    .loaded_document()
                    .registry
                    .history_query(&document.doc_id)?;
                let cursor = query.cursor as usize;
                let mut rows = Vec::with_capacity(query.entries.len() + 1);
                rows.push(crate::state::HistoryRow {
                    label: "Open".to_string(),
                    applied: cursor == 0,
                    current: cursor == 0,
                });
                for (index, entry) in query.entries.iter().enumerate() {
                    rows.push(crate::state::HistoryRow {
                        label: entry.label.clone(),
                        applied: index < cursor,
                        current: index + 1 == cursor,
                    });
                }
                Some(rows)
            })
            .unwrap_or_default();
        self.state.update(cx, |state, _| state.history = rows);
    }

    /// Apply or revert history until the requested cursor is current.
    pub(crate) fn jump_history(&mut self, target: u64, cx: &mut Context<Self>) {
        let result = (|| -> Result<(), String> {
            let doc_id = self.active_doc_id()?;
            let cursor = |app: &Self| {
                app.sessions
                    .get(&doc_id)
                    .and_then(|session| {
                        session
                            .loaded_document()
                            .registry
                            .get_history_cursor(&doc_id)
                    })
                    .unwrap_or(0) as u64
            };
            let mut guard = 0usize;
            while cursor(self) > target {
                let before = cursor(self);
                self.step_history(true, cx)?;
                guard += 1;
                if cursor(self) == before || guard > 4096 {
                    break;
                }
            }
            while cursor(self) < target {
                let before = cursor(self);
                self.step_history(false, cx)?;
                guard += 1;
                if cursor(self) == before || guard > 4096 {
                    break;
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.set_status_error(None, cx),
            Err(error) => self.set_status_error(Some(format!("History: {error}")), cx),
        }
        self.project_history(cx);
        cx.notify();
    }

    pub(crate) fn jump_history_action(&mut self, target: u64, cx: &mut Context<Self>) {
        self.jump_history(target, cx);
    }

    /// Step the active document's unified history cursor once.
    ///
    /// `Ok(false)` means the cursor did not move (bottom of undo history or top
    /// of redo history); the document version still bumps in core, so the
    /// cursor, not the version, is the no-op signal.
    pub(crate) fn step_history(
        &mut self,
        undo: bool,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        // A shortcut mid-gesture drops the in-flight stroke; it never commits it.
        self.cancel_canvas_gesture(cx);
        let doc_id = self.active_doc_id()?;
        let command = if undo { Command::Undo } else { Command::Redo };
        let (cursor_before, result) = {
            let session = self
                .sessions
                .get_mut(&doc_id)
                .ok_or_else(|| "active session is missing".to_string())?;
            let registry = &mut session.loaded_document_mut().registry;
            let cursor = registry.get_history_cursor(&doc_id).unwrap_or(0);
            let result = registry
                .apply_command(
                    &doc_id,
                    CommandEnvelope {
                        contract_version: CONTRACT_VERSION,
                        expected_version: None,
                        command,
                    },
                )
                .map_err(|error| format!("{}: {}", error.code, error.message))?;
            (cursor, result)
        };
        let cursor_after = self
            .sessions
            .get(&doc_id)
            .and_then(|session| {
                session
                    .loaded_document()
                    .registry
                    .get_history_cursor(&doc_id)
            })
            .unwrap_or(cursor_before);
        if cursor_after == cursor_before {
            return Ok(false);
        }
        self.sessions
            .mark_dirty(&doc_id)
            .map_err(|error| format!("dirty state failed: {error:?}"))?;
        // Metadata undos (opacity, adjustment, transforms) leave layer fields
        // changed even when a dirty-rect pixel handoff rides the same delta, so
        // the projection must always follow the cursor.
        self.project_layers(cx);
        let handoffs = result.pixel_patches.unwrap_or_default();
        if handoffs.is_empty() {
            self.refresh_active_canvas(cx)?;
        } else {
            // One handoff per restored layer: a destructive crop restores the
            // whole stack in a single step.
            for handoff in &handoffs {
                self.display_history_handoff(&doc_id, handoff, cx)?;
            }
        }
        self.project_history(cx);
        Ok(true)
    }

    fn display_history_handoff(
        &mut self,
        doc_id: &str,
        handoff: &PixelPatchHandoff,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if !self.can_patch_single_layer(doc_id) {
            return self.refresh_active_canvas(cx);
        }
        let patched = (|| -> Result<Vec<Arc<RenderImage>>, String> {
            let active = self
                .active_document
                .as_mut()
                .ok_or_else(|| "no active document".to_string())?;
            if active.doc_id != doc_id {
                return Err("active document changed during undo/redo".to_string());
            }
            let mut retired = active.canvas.discard_preview_patches();
            for tile in &handoff.tiles {
                let x = u32::try_from(tile.x).map_err(|_| "negative history tile x".to_string())?;
                let y = u32::try_from(tile.y).map_err(|_| "negative history tile y".to_string())?;
                let width =
                    u32::try_from(tile.w).map_err(|_| "history tile width overflow".to_string())?;
                let height = u32::try_from(tile.h)
                    .map_err(|_| "history tile height overflow".to_string())?;
                retired.extend(
                    active
                        .canvas
                        .update_region(x, y, width, height, &tile.data)
                        .map_err(|error| format!("history canvas tile: {error:?}"))?,
                );
            }
            Ok(retired)
        })();
        match patched {
            Ok(retired) => {
                self.retired_textures.extend(retired);
                cx.notify();
                Ok(())
            }
            Err(error) => {
                eprintln!(
                    "photrez: history tile patch failed: {error}; falling back to full refresh"
                );
                self.refresh_active_canvas(cx)
            }
        }
    }
}
