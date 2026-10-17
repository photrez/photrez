//! # app - the root Entity
//!
//! Owns: construction of the application's services and the state handed down
//! to `views/` (dependency injection). It is the only module allowed to know
//! the concrete wiring of `services/`.
//!
//! Must not own: canvas painting (`render/`), pointer policy (`input/`), or
//! any document logic - document truth lives in `photrez-core` and is reached
//! in-process through the bridge below.

use std::borrow::Cow;

use gpui_kit::{div, rgb, Context, IntoElement, ParentElement, Render, Styled, Window};

/// The `.ptz` `document.json` payload the bridge probe parses below: a real
/// payload of the production reader's shape, kept tiny. The dimensions the
/// reader yields from it are what the window displays.
const PROBE_DOCUMENT_JSON: &str = r#"{
  "id": "bridge-probe",
  "name": "Bridge Probe",
  "width": 1748,
  "height": 1240,
  "layers": []
}"#;

/// Dimensions of the probe document, produced by [`photrez_core`]'s
/// production `.ptz` JSON reader - not echoed from a constant here. This is
/// the in-process wiring proof: the number on screen comes from core.
pub fn probe_document_size() -> (f64, f64) {
    let doc = photrez_core::canonical_model::CanonicalDocument::from_ptz_document_json(
        PROBE_DOCUMENT_JSON,
    )
    .expect("probe document must parse");
    (doc.width, doc.height)
}

/// Registers Inter with GPUI's text system and reports whether it landed.
///
/// This is load-bearing rather than cosmetic: every measurement in the parity
/// baselines was taken from a window rendering Inter. If this silently falls
/// back to a system font, glyph advances differ and diffs fail for reasons
/// that have nothing to do with the port.
pub fn load_fonts(cx: &gpui_kit::App) {
    match cx
        .text_system()
        .add_fonts(vec![Cow::Borrowed(crate::INTER)])
    {
        Err(error) => eprintln!("photrez: FAILED to register Inter: {error:#}"),
        Ok(()) => {
            let names = cx.text_system().all_font_names();
            if names.iter().any(|name| name.trim() == "Inter") {
                eprintln!(
                    "photrez: Inter registered ({} families visible)",
                    names.len()
                );
            } else {
                eprintln!(
                    "photrez: WARNING - Inter not found after registration. \
                     DirectWrite sees {names:?}"
                );
            }
        }
    }
}

/// Root entity. For this scaffold it holds exactly one thing: the value the
/// bridge produced, which the window displays.
pub struct App {
    probe_size: (f64, f64),
}

impl App {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            probe_size: probe_document_size(),
        }
    }
}

impl Render for App {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let (w, h) = self.probe_size;
        div()
            .size_full()
            .bg(rgb(0x171717))
            .flex()
            .items_center()
            .justify_center()
            .font_family("Inter")
            .child(format!("photrez-core .ptz reader: {w} x {h}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bridge must return what the production reader parses, not a value
    /// of its own. RED proof: stub `probe_document_size()` to return
    /// `(1.0, 1.0)` and this test fails while the window number changes.
    #[test]
    fn bridge_returns_core_parsed_dimensions() {
        assert_eq!(probe_document_size(), (1748.0, 1240.0));
    }
}
