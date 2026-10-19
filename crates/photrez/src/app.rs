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
use std::time::{Duration, Instant};

use gpui_kit::{
    AppContext, Context, Entity, IntoElement, Render, Subscription, Window, WindowBounds,
};

use crate::render::canvas_bridge::CanvasImage;
use crate::services;
use crate::services::settings::{FileSettings, SettingsStore};
use crate::services::window_state::{self, WindowGeometry};
use crate::state::AppState;
use crate::views::workspace;

/// A drag-resize produces a bounds change per pixel of movement; the geometry
/// file is rewritten at most this often while that is happening. The settle
/// points (focus loss, quit) always write, so the final position is never lost.
const WRITE_INTERVAL: Duration = Duration::from_millis(500);

/// The `.ptz` `document.json` payload the bridge probe parses below: a real
/// payload of the production reader's shape, kept tiny. The dimensions the
/// reader yields from it are what the window displays.
#[allow(dead_code)] // only reachable from the bridge test until the shell wires it into a view
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
#[allow(clippy::expect_used)]
// the probe payload is a compile-time constant; a parse failure is a defect, not a runtime condition
#[allow(dead_code)] // only reachable from the bridge test until the shell wires it into a view
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

/// Root entity — the window's single GPUI root.
///
/// Holds the `AppState` that drives every region of the shell. At this stage
/// `AppState` is seeded entirely from `shot::active()`, so a `--shot` launch
/// renders the exact baseline state; `probe_document_size()` is kept as an
/// in-process wiring proof that `photrez-core`'s JSON reader works.
pub struct App {
    state: Entity<AppState>,
    /// The display snapshot of the demo document, built once at startup
    /// through `photrez-core` (`services::document`). The seam only converts
    /// and paints it; document truth stays in core. `None` is defined
    /// behaviour: the canvas area falls back to its flat colour.
    canvas: Option<CanvasImage>,
    /// The window geometry seen on the last frame. `render` is the one place
    /// handed a live window every frame, so it is where the geometry is
    /// sampled; the observers below only decide when it is written.
    live: Option<WindowGeometry>,
    /// The record already on disk, and when it was written: unchanged geometry
    /// is never rewritten and a drag-resize is throttled by [`WRITE_INTERVAL`].
    saved: Option<WindowGeometry>,
    saved_at: Option<Instant>,
    /// Where the record goes. `None` (no config directory) disables persistence
    /// without disabling the window.
    settings: Option<FileSettings>,
    /// Held for the window's lifetime: dropping a subscription cancels it.
    _window_watch: Vec<Subscription>,
}

impl App {
    /// Creates a new `App`, seeding `AppState` from the active shot id,
    /// building the demo document's display snapshot through core, and
    /// subscribing to the window geometry it will persist.
    pub fn new(
        window: &mut Window,
        settings: Option<FileSettings>,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = AppState::seeded(crate::shot::active());
        let (width, height, rgba) = services::document::blank_white_document();
        let canvas = match CanvasImage::from_rgba(width, height, rgba) {
            Ok(canvas) => Some(canvas),
            Err(error) => {
                eprintln!("photrez: the demo document was refused by the display seam: {error:?}");
                None
            }
        };

        let mut window_watch = Vec::new();
        // Resizes (and any bounds notification) are throttled; the settle
        // points always write.
        window_watch.push(cx.observe_window_bounds(window, |this, window, _cx| {
            this.remember_bounds(window);
            this.persist_bounds(false);
        }));
        // Focus loss is the settle point that follows a move, which the
        // platform reports no event for: sample and write there.
        window_watch.push(cx.observe_window_activation(window, |this, window, _cx| {
            this.remember_bounds(window);
            this.persist_bounds(true);
        }));
        window_watch.push(cx.on_app_quit(|this, _cx| {
            this.persist_bounds(true);
            async {}
        }));

        Self {
            state: cx.new(|_| state),
            canvas,
            live: Some(restorable_geometry(window)),
            saved: None,
            saved_at: None,
            settings,
            _window_watch: window_watch,
        }
    }

    fn remember_bounds(&mut self, window: &Window) {
        self.live = Some(restorable_geometry(window));
    }

    /// Writes the last seen geometry when it differs from the record already on
    /// disk. `settled` marks the end of a gesture (focus loss, quit), where the
    /// throttle must not skip the final position.
    fn persist_bounds(&mut self, settled: bool) {
        let Some(geometry) = self.live else {
            return;
        };
        if self.saved == Some(geometry) {
            return;
        }
        if !settled
            && self
                .saved_at
                .is_some_and(|at| at.elapsed() < WRITE_INTERVAL)
        {
            return;
        }
        if let Some(settings) = &self.settings {
            settings.write(window_state::KEY, &window_state::encode(geometry));
        }
        self.saved = Some(geometry);
        self.saved_at = Some(Instant::now());
    }
}

/// The window's restorable geometry: a normal window's bounds, or a maximized
/// window's restore bounds plus its state. `WindowBounds::Maximized` carries
/// the restore rect by contract, so a maximized window is never remembered as
/// a normal window the size of the screen.
///
/// The shell exposes no fullscreen command; a fullscreen window's restore rect
/// is the honest record of where it was before.
fn restorable_geometry(window: &Window) -> WindowGeometry {
    match window.window_bounds() {
        WindowBounds::Windowed(bounds) => WindowGeometry::of(bounds, false),
        WindowBounds::Maximized(bounds) => WindowGeometry::of(bounds, true),
        WindowBounds::Fullscreen(bounds) => WindowGeometry::of(bounds, false),
    }
}

impl Render for App {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The geometry is sampled here because this is the only place the root
        // is handed a live window every frame.
        self.remember_bounds(window);
        // Delegate to the workspace renderer.
        workspace::render(self.state.read(cx), window, self.canvas.as_ref())
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

    /// `App::new` seeds `AppState` from `shot::active()`.
    #[test]
    fn seeded_state_matches_shot() {
        let state = AppState::seeded(Some("dock-closed"));
        assert!(!state.dock_open);
    }
}
