//! The Adjustments panel: its appearance.
//!
//! # Geometry: 272 wide, x 833..1104, y 84..795
//!
//! `properties_panel` shares this InspectorDock slot. The panel occupies
//! the leftmost 272px of `dock-adjustments.png` below its 38px tab bar
//! (crop x 0..271, y 38..749). Values are read from the PNG, not class names.
//!
//! ```text
//! tab bar bottom border  crop y=37        #333333, full width
//! section 1              crop y=37..137   "Selected Layer"
//!   SectionHeader        text crop y=51..63
//!   thumb box            crop y=75..125, x 13..253   FIELD fill, #333333 border
//!     thumb 34x34        crop x 22..55, y 83..117    checkerboard, #333333 border
//!     name               crop y 87..98                 11.5px medium, TEXT
//!     type + size        crop y 104..113               10.5px, TEXT_DIM
//! section 1 divider      crop y=137
//! section 2              crop y=137..384  "Basic"
//!   SectionHeader        text crop y=151..163 ("Basic" x38..65)
//!     reset icon         ink x243..247, y154..161      (disabled, 40% opacity)
//!   slider rows          labels crop y185..196 / 221..230 / 257..266
//!     tracks (4px)       crop y188..191 / 224..227 / 260..263
//!     thumbs (12px)      centred at crop x=142, y=189.5 / 225.5 / 261.5
//!     value boxes        crop x215..254, y179..200 / 216..235 / 252..271
//!       "0"              ink x241..246 in each box
//!   description          crop y291..300 + y306..313    (two wrapped lines)
//!   StatusHint box       crop y326..372; icon ink x26..40, y338..349;
//!                        text y338..347 + y353..362
//! section 2 divider      crop y=384
//! 5 collapsed sections   crop y=384..599, 42px + 1px border each:
//!   icon box             crop x16..32 (icon 15px, ink x19..31)
//!   label                12.5px TEXT from crop x~43
//!   chevron              ink x240..246
//! panel content ends     crop y=599 — the last 113px of the 712px
//!                        column is empty PANEL; `flex-1` on the scroll
//!                        column is all that is needed.
//! ```
//!
//! # Captured state
//!
//! The dock selects this view unless the inspector tab is "presets";
//! the capture has Adjustments active (underline at crop y35..36, x79..168).
//!
//! "Background" is selected: a 100×100 raster without `imageBitmap`; thumb
//! empty, no-pixels hint from `basicStatusText()`, sliders at 0, reset
//! disabled. Five closed `CollapsibleSection`s render only 42px headers;
//! `COMING_SOON_SECTIONS` descriptions do not render.
//!
//! # Known gaps
//!
//! * `LayerThumb` alternates `rgba(255,255,255,0.05)` and
//!   `rgba(255,255,255,0.1)` in 6px cells over FIELD (`#252525`), yielding
//!   `#303030` / `#3b3b3b`. GPUI's one-colour `checkerboard()` uses their
//!   50/50 mean (`#353535`, [`CHECKER_MEAN`]); this layer has no pixels.
//! * At rest (no pending adjustment), reset is disabled;
//!   `disabled:opacity-40` paints at 40%. Omitted interaction styles:
//!   `hover:bg-white/[0.045]`, `hover:text-editor-text`,
//!   `hover:bg-white/[0.03]`, `hover:scale-110`, and status
//!   `transition-colors`.
//! * GPUI lacks `font-variant-numeric`; Inter defaults replace
//!   `tabular-nums`. Static ports cannot reproduce icon `Tooltip` wrappers.
//! * Standard gaps: no letter-spacing, clip-path, translate/z-index/scale;
//!   two-stop gradients only (five-stop saturation uses
//!   `ui::gradient_segments`); padding rounds to whole px; odd-height
//!   centring can differ by ±1px (22px value boxes in 28px rows).
//!
//! # Border and gutter resolution
//!
//! * A global border colour outranks every per-element border the panel asks
//!   for — the divider, field-border and accent borders included.
//!   Borders paint `#333333` ([`color::BORDER`]);
//!   all seven divider rows and every box border scan as `#333333`.
//! * Every scrollable area reserves 6px for its scrollbar gutter,
//!   with or without overflow; the scroll column's
//!   `.pr(px(6.))` leaves content at crop x=254, not x=259.

use crate::components::ui;
use crate::i18n::t;
use crate::icons;
use crate::tokens::color;

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, px, relative, Div, FontWeight, Hsla, TestSupportExt};

/// InspectorDock's width: 272px.
#[expect(
    dead_code,
    reason = "documents baseline geometry; parent dock sets size via w_full()/h_full()"
)]
const PANEL_W: f32 = 272.0;

/// Content height below the 38px tab bar: 750 - 38.
#[expect(
    dead_code,
    reason = "documents baseline geometry; parent dock sets size via w_full()/h_full()"
)]
const PANEL_H: f32 = 712.0;

/// `px-3` — each section's horizontal inset, measured as the first
/// ink pixel at crop x=12.
const PAD_X: f32 = 12.0;

/// `py-2.5` — the inset around each section's content.
const PAD_Y: f32 = 10.0;

/// `mt-2` between a `SectionHeader` and the block under it.
const HEADER_GAP: f32 = 8.0;

/// 10px between the thumbnail and its text, and between the
/// slider and the value box.
const ROW_GAP: f32 = 10.0;

/// 8px inside the "Selected Layer" box.
const BOX_PAD: f32 = 8.0;

/// The layer thumbnail's canvas is 34x34.
const THUMB: f32 = 34.0;

/// 2px between the layer name and its type line — the type line carries a
/// 2px top margin rather than a flex gap.
const TYPE_MT: f32 = 2.0;

/// 4px / 3px corner radii.
const R4: f32 = 4.0;
const R3: f32 = 3.0;

/// 16px / 15px — a section header's icon box and icon.
const HDR_ICON_BOX: f32 = 16.0;
const HDR_ICON: f32 = 15.0;

/// `text-[12.5px] font-medium` — `SectionHeader`'s label.
const HDR_TEXT: f32 = 12.5;

/// 11.5px / 10.5px — the layer name and its type line.
const NAME_TEXT: f32 = 11.5;
const TYPE_TEXT: f32 = 10.5;

/// 58px — a slider row's label.
const SLIDER_LABEL_W: f32 = 58.0;

/// `text-[12px]` — the slider row's label.
const SLIDER_LABEL: f32 = 12.0;

/// 28px minimum height — the slider row, and 18px — the slider's
/// wrapper.
const SLIDER_ROW_H: f32 = 28.0;
const SLIDER_WRAP_H: f32 = 18.0;

/// 40x22px and 6px inset — the slider's value box.
const VALUE_W: f32 = 40.0;
const VALUE_H: f32 = 22.0;
const VALUE_PAD_X: f32 = 6.0;

/// `text-[11px]` — the value box's readout.
const VALUE_TEXT: f32 = 11.0;

/// `mt-1` above the "Drag to preview…" paragraph, and its
/// `text-[11px]`.
const DESC_MT: f32 = 4.0;
const DESC_TEXT: f32 = 11.0;

/// 8px gap, 10px horizontal and 8px vertical padding, 14px icon — the status
/// hint box, its icon and its inset.
const HINT_GAP: f32 = 8.0;
const HINT_PAD_X: f32 = 10.0;
const HINT_PAD_Y: f32 = 8.0;
const HINT_ICON: f32 = 14.0;

/// 11px — the status hint's message.
const HINT_TEXT: f32 = 11.0;

/// 42px header row, 16px inset, 10px gap, 16px icon box, 15px icon,
/// 12.5px label, 16px chevron — a collapsible section's header row
/// and its parts.
const COLLAPSE_H: f32 = 42.0;
const COLLAPSE_PX: f32 = 16.0;
const COLLAPSE_GAP: f32 = 10.0;
const COLLAPSE_ICON_BOX: f32 = 16.0;
const COLLAPSE_ICON: f32 = 15.0;
const COLLAPSE_TEXT: f32 = 12.5;
const CHEVRON: f32 = 16.0;

/// 20px / 14px — the Basic header's reset button and its
/// icon.
const RESET: f32 = 20.0;
const RESET_ICON: f32 = 14.0;

const CHECKER_MEAN: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.208,
    a: 1.0,
};

// Coming-soon icons use raw Tailwind colors, not editor tokens.

/// `text-sky-400` — measured core: `#00bcff`.
const SKY_400: Hsla = Hsla {
    h: 0.543791,
    s: 1.0,
    l: 0.5,
    a: 1.0,
};

/// `text-sky-300` — measured core: `#74d4ff`.
const SKY_300: Hsla = Hsla {
    h: 0.551559,
    s: 1.0,
    l: 0.727451,
    a: 1.0,
};

/// `text-amber-400` — measured core: `#fcb701` (within anti-aliasing
/// tolerance).
const AMBER_400: Hsla = Hsla {
    h: 0.120915,
    s: 1.0,
    l: 0.5,
    a: 1.0,
};

/// `text-emerald-400` — measured core: `#01d291` (within anti-aliasing
/// tolerance).
const EMERALD_400: Hsla = Hsla {
    h: 0.448113,
    s: 1.0,
    l: 0.415686,
    a: 1.0,
};

/// Formats an adjustment the way the value box shows it.
pub fn format_display_value(value: i32) -> String {
    if value > 0 {
        format!("+{}", value)
    } else {
        value.to_string()
    }
}

pub fn render(
    state: &gpui_kit::Entity<crate::state::AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let (layer_name, layer_type, layer_w, layer_h) = {
        let s = state.read(cx);
        (
            s.active_layer_name.clone(),
            match s.selected_layer_type.as_deref() {
                Some("shape") => "Shape",
                Some("text") => "Text",
                _ => "Image",
            },
            s.active_layer_width,
            s.active_layer_height,
        )
    };
    v_flex()
        .w_full()
        .h_full()
        .overflow_hidden()
        .bg(color::PANEL)
        .child(
            v_flex()
                .flex_1()
                .overflow_hidden()
                // The scrollbar gutter reserves 6px on every
                // scrollable area; GPUI has no
                // scrollbars, so reserve that space explicitly.
                .child(selected_layer_section(
                    &layer_name,
                    layer_type,
                    layer_w,
                    layer_h,
                ))
                .child(basic_section(state, cx))
                // All five `COMING_SOON_SECTIONS` are collapsed in the
                // capture, so only their header rows render.
                .child(coming_soon_section(
                    icons::with_stroke(icons::SPLINE, 1.75),
                    color::TEXT_DIM,
                    &t("adjustments.toneCurve"),
                ))
                .child(coming_soon_section(
                    icons::with_stroke(icons::PALETTE, 1.75),
                    SKY_400,
                    &t("adjustments.hslColor"),
                ))
                .child(coming_soon_section(
                    icons::with_stroke(icons::SWATCH, 1.75),
                    AMBER_400,
                    &t("adjustments.colorGrading"),
                ))
                .child(coming_soon_section(
                    icons::with_stroke(icons::SPARKLES, 1.75),
                    SKY_300,
                    &t("adjustments.detail"),
                ))
                .child(coming_soon_section(
                    icons::with_stroke(icons::APERTURE, 1.75),
                    EMERALD_400,
                    &t("adjustments.lensCorrections"),
                )),
        )
}

mod sections;
use sections::{basic_section, coming_soon_section, selected_layer_section};

#[cfg(test)]
mod tests {
    use super::format_display_value;

    /// The positive assertion detects a reversed comparison.
    #[test]
    fn format_display_value_adds_plus_for_positive() {
        assert_eq!(format_display_value(0), "0");
        assert_eq!(format_display_value(5), "+5");
        assert_eq!(format_display_value(-5), "-5");
        assert_eq!(format_display_value(100), "+100");
    }
}
