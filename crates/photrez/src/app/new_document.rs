//! New Document dialog: category tabs, preset grid and
//! the name/size/unit/background form.
//!
//! The form keeps its size in pixels, the same way the document store counts:
//! the width and
//! height fields only carry display text in the chosen unit, and a unit switch
//! reformats the stored pixel value. A preset copies its name and size into
//! the form; a second press within the double-click window creates the
//! document immediately.

use std::time::{Duration, Instant};

use gpui_kit::{Context, KeyDownEvent, Window};

use crate::state::{DialogId, NewDocCategory, NewDocField, NewDocumentSession};
use crate::units::DimensionUnit;

use super::App;

/// A browser double click reports a click count; native clicks carry no
/// reliable count, so the second press inside this window counts as one.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

impl App {
    pub(crate) fn open_new_document(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.open_menu = None;
            state.new_document = Some(NewDocumentSession::default());
            state.active_dialog = Some(DialogId::NewDocument);
        });
        self.pending_new_document_focus = true;
        cx.notify();
    }

    pub(crate) fn close_new_document(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.new_document = None;
            state.active_dialog = None;
        });
        cx.notify();
    }

    /// A backdrop press closes an open dropdown before the dialog itself.
    pub(crate) fn dismiss_new_document_panel(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self
            .state
            .read(cx)
            .new_document
            .as_ref()
            .is_some_and(|session| session.unit_open || session.background_open);
        if open {
            self.state.update(cx, |state, _| {
                if let Some(session) = state.new_document.as_mut() {
                    session.unit_open = false;
                    session.background_open = false;
                }
            });
            cx.notify();
        }
        open
    }

    pub(crate) fn select_new_doc_category(
        &mut self,
        category: NewDocCategory,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| {
            if let Some(session) = state.new_document.as_mut() {
                session.category = category;
            }
        });
        cx.notify();
    }

    pub(crate) fn focus_new_doc_field(
        &mut self,
        field: NewDocField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .state
            .read(cx)
            .new_document
            .as_ref()
            .and_then(|session| session.field);
        if let Some(current) = current {
            self.commit_new_document_dimension(current, cx);
        }
        self.state.update(cx, |state, _| {
            if let Some(session) = state.new_document.as_mut() {
                session.field = Some(field);
                session.unit_open = false;
                session.background_open = false;
            }
        });
        window.focus(&self.new_document_focus, cx);
        cx.notify();
    }

    /// Apply a preset to the form, creating the document on the second
    /// press. Returns `true`
    /// when the press created a document.
    pub(crate) fn click_new_document_preset(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut session) = self.state.read(cx).new_document.clone() else {
            return false;
        };
        let Some(preset) = session.category.presets().get(index).copied() else {
            return false;
        };
        session.apply_preset(&preset);
        let now = Instant::now();
        let double = session
            .last_preset
            .is_some_and(|(last, at)| last == index && now.duration_since(at) <= DOUBLE_CLICK);
        session.last_preset = if double { None } else { Some((index, now)) };
        self.state.update(cx, |state, _| {
            if let Some(current) = state.new_document.as_mut() {
                *current = session;
            }
        });
        if double {
            self.create_new_document(cx);
        } else {
            cx.notify();
        }
        double
    }

    fn commit_new_document_dimension(&mut self, field: NewDocField, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            if let Some(session) = state.new_document.as_mut() {
                session.commit_dimension(field);
            }
        });
    }

    /// The dialog's Create action and the form's Enter key.
    pub(crate) fn create_new_document(&mut self, cx: &mut Context<Self>) {
        self.commit_new_document_dimension(NewDocField::Width, cx);
        self.commit_new_document_dimension(NewDocField::Height, cx);
        let Some(session) = self.state.read(cx).new_document.clone() else {
            return;
        };
        let result = self.create_new_named_document(
            session.name.clone(),
            session.width_px,
            session.height_px,
            session.background,
            cx,
        );
        match result {
            Ok(()) => {
                self.state.update(cx, |state, _| state.new_document = None);
                self.set_status_error(None, cx);
            }
            Err(error) => self.set_status_error(Some(format!("New: {error}")), cx),
        }
    }

    pub(crate) fn new_document_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        let panel_open = self
            .state
            .read(cx)
            .new_document
            .as_ref()
            .is_some_and(|session| session.unit_open || session.background_open);
        if key == "escape" {
            if panel_open {
                self.dismiss_new_document_panel(cx);
            } else {
                self.close_new_document(cx);
            }
            return true;
        }
        match key {
            "enter" => {
                self.create_new_document(cx);
                true
            }
            "tab" => {
                let next = self.state.update(cx, |state, _| {
                    let session = state.new_document.as_mut()?;
                    let committed = session.field;
                    if let Some(field) = committed {
                        session.commit_dimension(field);
                    }
                    let next = match committed {
                        None | Some(NewDocField::Name) => NewDocField::Width,
                        Some(NewDocField::Width) => NewDocField::Height,
                        Some(NewDocField::Height) => NewDocField::Name,
                    };
                    session.field = Some(next);
                    Some(next)
                });
                if next.is_some() {
                    cx.notify();
                    true
                } else {
                    false
                }
            }
            "backspace" => {
                self.state.update(cx, |state, _| {
                    let Some(session) = state.new_document.as_mut() else {
                        return;
                    };
                    match session.field {
                        Some(NewDocField::Name) => {
                            session.name.pop();
                        }
                        Some(NewDocField::Width) => {
                            session.width_text.pop();
                        }
                        Some(NewDocField::Height) => {
                            session.height_text.pop();
                        }
                        None => {}
                    }
                });
                cx.notify();
                true
            }
            _ => {
                let text = event
                    .keystroke
                    .key_char
                    .clone()
                    .unwrap_or_else(|| event.keystroke.key.clone());
                if text.chars().count() != 1 || text.chars().any(char::is_control) {
                    return false;
                }
                let character = text.chars().next().unwrap_or_default();
                self.state.update(cx, |state, _| {
                    let Some(session) = state.new_document.as_mut() else {
                        return;
                    };
                    match session.field {
                        Some(NewDocField::Name) => session.name.push(character),
                        Some(NewDocField::Width)
                            if character.is_ascii_digit()
                                || (character == '.' && session.unit != DimensionUnit::Px) =>
                        {
                            session.width_text.push(character);
                        }
                        Some(NewDocField::Width) => {}
                        Some(NewDocField::Height)
                            if character.is_ascii_digit()
                                || (character == '.' && session.unit != DimensionUnit::Px) =>
                        {
                            session.height_text.push(character);
                        }
                        Some(NewDocField::Height) => {}
                        None => {}
                    }
                });
                cx.notify();
                true
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn new_document_form_for_test(
        &self,
        cx: &gpui_kit::App,
    ) -> Option<NewDocumentSession> {
        self.state.read(cx).new_document.clone()
    }
}
