use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::Icon;
use gpui_kit::prelude::*;
use gpui_kit::{div, hsla, px, relative, Div, FontWeight, Hsla, Stateful};

use crate::components::ui;
use crate::i18n::t;
use crate::icons;
use crate::tokens::color;

use super::title_bar;
use super::{GAP_2, R4, R6};
/// The About dialog's own measurements. `AboutDialog.tsx:44-128` carries the
/// class names most of these come from; the departures \u{2014} the brand mark
/// drawn at 44px instead of a 32px glyph inside a 48px tile, and the 12px
/// rhythm between the three blocks \u{2014} are the rebuild's, not the source's.
const ABOUT_MARK: f32 = 44.0;

/// The body column's rhythm between hero, spec panel and link row.
const ABOUT_GAP: f32 = 12.0;

/// `gap-3.5` \u{2014} mark to identity block.
const ABOUT_HERO_GAP: f32 = 14.0;

/// `p-2.5` and `gap-y-1.5` on the spec panel, whose `rounded-[6px]` is the
/// shared R6.
const ABOUT_PANEL_PAD: f32 = 10.0;
const ABOUT_PANEL_ROW_GAP: f32 = 6.0;

/// `h-7` link buttons with a `size-3.5` icon at `gap-1.5`.
const ABOUT_LINK_H: f32 = 28.0;
const ABOUT_ICON: f32 = 14.0;

/// `px-1.5 py-0.5 rounded-[4px]` on the version pill.
const ABOUT_PILL_PX: f32 = 6.0;
const ABOUT_PILL_PY: f32 = 2.0;

/// `bg-[#151516]` \u{2014} the spec panel's fill (`AboutDialog.tsx:47,87`), a
/// component literal two steps darker than PANEL, so it is pinned here rather
/// than joining `tokens.rs`.
const ABOUT_PANEL_BG: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.0824,
    a: 1.0,
};

/// `bg-editor-accent/10` \u{2014} the version pill's fill (`AboutDialog.tsx:76`),
/// alongside the crop bar's `ACCENT_15` in `tokens.rs`.
const ABOUT_PILL_FILL: Hsla = Hsla {
    h: color::ACCENT.h,
    s: color::ACCENT.s,
    l: color::ACCENT.l,
    a: 0.10,
};

/// `border-editor-accent/40` \u{2014} the version pill's edge (`AboutDialog.tsx:76`)
/// and the link buttons' hover border (`:109,119`).
const ABOUT_ACCENT_40: Hsla = Hsla {
    h: color::ACCENT.h,
    s: color::ACCENT.s,
    l: color::ACCENT.l,
    a: 0.40,
};

/// The four spec rows' labels, catalogue-routed like the web dialog's.
pub(super) const ABOUT_SPEC_LABELS: [&str; 4] = [
    "dialogs.about.coreEngine",
    "dialogs.about.rasterizer",
    "dialogs.about.runtimeShell",
    "dialogs.about.license",
];

/// The spec values stay literals: the embedded catalogues are byte-locked to
/// the reference (see the i18n module), so a native-only string cannot join
/// them. The two rows the web dialog fills with "WebGL 2D" and "Tauri v2 +
/// SolidJS" name the shell this port replaces; these name what actually runs.
pub(super) const ABOUT_SPEC_VALUES: [&str; 4] = [
    "Rust (photrez-core)", // i18n-exempt: tech value, literal in the reference
    "CPU tile renderer",   // i18n-exempt: tech value, not in catalog
    "Rust + GPUI",         // i18n-exempt: tech value, not in catalog
    "AGPLv3 Open-Source",  // i18n-exempt: tech value, literal in the reference
];

/// The two shipped catalogues' names for themselves, so the line reads the
/// same under either locale. Comma-separated rather than a middle dot: the
/// embedded face has no U+00B7, and the coverage gate reviews catalogue text,
/// not native literals.
pub(super) const ABOUT_LANGUAGES: &str = "English, Bahasa Indonesia"; // i18n-exempt: endonyms, not in catalog

/// The link row's labels \u{2014} literals in the web dialog
/// (`AboutDialog.tsx:114,126`), not catalogue keys.
pub(super) const ABOUT_LINK_GITHUB: &str = "GitHub Repository"; // i18n-exempt: not in catalog
pub(super) const ABOUT_LINK_WEBSITE: &str = "Official Website"; // i18n-exempt: not in catalog

/// Where the two link buttons lead (`AboutDialog.tsx:108,118`).
const ABOUT_LINK_GITHUB_URL: &str = "https://github.com/photrez/photrez"; // i18n-exempt: URL, not text
const ABOUT_LINK_WEBSITE_URL: &str = "https://photrez.github.io"; // i18n-exempt: URL, not text

/// `v{version}` \u{2014} the web dialog asks Tauri's `getVersion()` at mount and
/// falls back to a hardcoded "0.1.0"; here the crate's own version is the only
/// source, so there is no fallback string to go stale.
pub(super) fn about_version() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

/// `AboutDialog`, rebuilt for the native shell rather than transcribed from
/// `dialog-about.png`: the web dialog's hero, spec grid and link row survive
/// as the structure, and three things change with the shell. The mark is the
/// real brand raster at 44px instead of a glyph in a tile; the version comes
/// from `CARGO_PKG_VERSION` instead of a runtime round-trip; and the locale
/// line plus the two corrected stack rows state what ships here rather than
/// what the replaced shell ran. The chrome above and below is shared and
/// untouched, and the capture difference is a recorded decision, not drift.
pub(super) fn about_body() -> Div {
    v_flex()
        .w_full()
        .h_full()
        .justify_center()
        .gap(px(ABOUT_GAP))
        .child(about_hero())
        .child(about_specs())
        .child(about_links())
}

/// `flex items-center gap-3.5` \u{2014} brand mark beside the identity block.
fn about_hero() -> Div {
    h_flex()
        .w_full()
        .items_center()
        .gap(px(ABOUT_HERO_GAP))
        .child(title_bar::logo(ABOUT_MARK))
        .child(
            v_flex()
                .flex_1()
                .min_w(px(0.0))
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(GAP_2))
                        .child(
                            div()
                                .text_size(px(15.0))
                                .font_family(ui::FONT_UI)
                                .font_weight(FontWeight(700.))
                                .line_height(relative(1.35))
                                .text_color(color::WHITE)
                                .child("Photrez".to_string()), // i18n-exempt: product name
                        )
                        .child(about_version_pill()),
                )
                .child(ui::clip(
                    div()
                        .mt(px(2.0))
                        .text_size(px(11.0))
                        .font_family(ui::FONT_UI)
                        .line_height(relative(1.35))
                        .text_color(color::TEXT_DIM)
                        .child(t("dialogs.about.tagline")),
                ))
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(10.0))
                        .font_family(ui::FONT_UI)
                        .line_height(relative(1.35))
                        .text_color(color::TEXT_DIM)
                        .child(ABOUT_LANGUAGES.to_string()),
                ),
        )
}

/// `rounded-[4px] border border-editor-accent/40 bg-editor-accent/10 px-1.5
/// py-0.5 text-[10px] font-semibold text-editor-accent` (`AboutDialog.tsx:76`).
fn about_version_pill() -> Div {
    div()
        .rounded(px(R4))
        .border_1()
        .border_color(ABOUT_ACCENT_40)
        .bg(ABOUT_PILL_FILL)
        .px(px(ABOUT_PILL_PX))
        .py(px(ABOUT_PILL_PY))
        .child(
            div()
                .text_size(px(10.0))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(600.))
                .line_height(relative(1.3))
                .text_color(color::ACCENT)
                .child(about_version()),
        )
}

/// `rounded-[6px] border border-editor-field-border bg-[#151516] p-2.5` with a
/// two-column `gap-y-1.5` grid inside (`AboutDialog.tsx:87-101`).
fn about_specs() -> Div {
    v_flex()
        .w_full()
        .rounded(px(R6))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(ABOUT_PANEL_BG)
        .p(px(ABOUT_PANEL_PAD))
        .gap(px(ABOUT_PANEL_ROW_GAP))
        .child(about_spec_row(0, 1))
        .child(about_spec_row(2, 3))
}

/// One `grid-cols-2` row: two equal cells with no column gap \u{2014} the split
/// the web dialog renders.
fn about_spec_row(left: usize, right: usize) -> Div {
    h_flex()
        .w_full()
        .child(about_spec_cell(left))
        .child(about_spec_cell(right))
}

/// `Label: value` on one line: the label in the caption colour, the value
/// carrying `font-medium text-editor-text`. The label is pinned and the cell
/// clips, so the first cell's longer value cannot paint over the second.
fn about_spec_cell(i: usize) -> Div {
    ui::clip(h_flex().flex_1().gap(px(4.0)))
        .child(ui::pin(
            div()
                .text_size(px(11.0))
                .font_family(ui::FONT_UI)
                .line_height(relative(1.35))
                .text_color(color::TEXT_DIM)
                .child(format!("{}:", t(ABOUT_SPEC_LABELS[i]))),
        ))
        .child(
            div()
                .text_size(px(11.0))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .line_height(relative(1.35))
                .text_color(color::TEXT)
                .child(ABOUT_SPEC_VALUES[i].to_string()),
        )
}

/// `flex items-center gap-2` \u{2014} the two `h-7 flex-1` link buttons
/// (`AboutDialog.tsx:105-128`).
fn about_links() -> Div {
    h_flex()
        .w_full()
        .gap(px(GAP_2))
        .child(about_link_button(
            icons::GITHUB,
            ABOUT_LINK_GITHUB,
            ABOUT_LINK_GITHUB_URL,
        ))
        .child(about_link_button(
            icons::GLOBE,
            ABOUT_LINK_WEBSITE,
            ABOUT_LINK_WEBSITE_URL,
        ))
}

/// `h-7 flex-1 rounded-[4px] border border-editor-field-border bg-editor-field
/// text-[11px] font-medium` with a `size-3.5` icon at `gap-1.5`.
///
/// The dialog's one wired control: `cursor-pointer`, `hover:bg-white/[0.08]`,
/// `hover:border-editor-accent/40` and the web button's open-in-browser click
/// (`AboutDialog.tsx:106-115`). `url` is both the hit-test id and the target,
/// so a button cannot drift from where it leads. Returns `Stateful<Div>` for
/// `.id(..)`, like the dock's tab button.
fn about_link_button(icon: &'static [u8], label: &str, url: &'static str) -> Stateful<Div> {
    h_flex()
        .id(url)
        .flex_1()
        .min_w(px(0.0))
        .h(px(ABOUT_LINK_H))
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .rounded(px(R4))
        .border_1()
        .border_color(color::FIELD_BORDER)
        .bg(color::FIELD)
        .cursor_pointer()
        // No token holds 8% white (the scale carries 5 and 10); inline like the
        // dock tab's 2%. The alpha blends over FIELD exactly as the CSS does.
        .hover(|s| {
            s.bg(hsla(0.0, 0.0, 1.0, 0.08))
                .border_color(ABOUT_ACCENT_40)
        })
        .on_click(move |_, _, cx| cx.open_url(url))
        .child(
            Icon::default()
                .data(icon)
                .size(px(ABOUT_ICON))
                .text_color(color::ICON),
        )
        .child(ui::pin(
            div()
                .text_size(px(11.0))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .line_height(relative(1.35))
                .text_color(color::TEXT)
                .child(label.to_string()),
        ))
}
