//! # views - screen-level regions
//!
//! Owns: composition of screen regions (title bar, tool rail, document tabs,
//! option bar, docks, status bar, canvas area) and the `Entity` state they
//! hold, handed down from `app/`.
//!
//! Must not own: reusable visual vocabulary (that is `components/`), canvas
//! painting (`display/`), or document truth (`photrez-core`).

pub mod adjustments_panel;
pub mod brush_menu;
pub mod canvas_surface;
pub mod color_picker;
pub mod context_menus;
pub mod dialogs;
pub mod document_tabs;
pub mod empty_state;
mod file_drop_surface;
mod gradient_badge;
pub mod history;
pub mod layers;
pub mod loading;
pub mod menus;
pub mod option_bar;
pub mod properties_panel;
pub mod right_dock;
pub mod status_bar;
pub mod title_bar;
pub mod toasts;
pub mod tool_rail;
pub mod tooltips;
mod transform_hud;
pub mod workspace;
