//! Design tokens for the native Photrez shell.
//!
//! Source of truth: the design's original theme token block.
//! The (stale) `docs/reference/design-tokens.md` disagrees with the shipped
//! code in places - e.g. it describes a 35px diagonal clip-path swatch, while
//! the tool rail actually renders two overlapping 28px circles. This file
//! follows the shipped code, not the doc.
//!
//! # Why HSL and not OKLCH
//!
//! GPUI's paint pipeline is `Hsla`-native; there is no OKLCH path (no gamut
//! mapping, no `(0,0,0)`-chroma shortcut). Every colour below was computed as:
//!
//!   1. OKLCH -> linear sRGB  (OKLab inverse: l_ = L + ...a + ...b, then cube)
//!   2. linear -> sRGB  (piecewise transfer function, clamped to gamut)
//!   3. sRGB -> HSL     (what gpui actually stores)
//!
//! Step 1 was verified against anchors: `oklch(0.5 0 0)` -> 99,
//! `oklch(0.6 0 0)` -> 128, `oklch(1 0 0)` -> 255.
//!
//! Clamping matters only for the two chromatic tokens (ACCENT, BRAND): OKLCH
//! chroma 0.15/0.20 at those hues sits marginally outside sRGB, so the previous
//! renderer clips them too. We clip identically, so the rendered colour matches.
//!
//! # Known fidelity gaps vs. the retired web stylesheet
//!
//! * OKLCH is not representable, so the ramp is only as smooth as sRGB allows.
//!   For this palette chroma is 0 for every neutral, so the gap is zero.
//! * The theme's radius scale is a base value minus a pixel offset, and it is
//!   never used by the tool rail, which hardcodes a 5px corner.

/// Design tokens from the design's original theme block,
/// keyed by the role each colour paints.
///
/// Import as `gpui_kit::Hsla` rather than `gpui::Hsla`: this crate depends only
/// on `gpui-kit`, which re-exports GPUI at its root (`pub use ::gpui::*`) but
/// does not make a `gpui` crate available to dependents.
pub mod color {
    use gpui_kit::Hsla;

    mod variants;

    pub use variants::{
        ACCENT_15, BLACK_40, BLACK_55, BLUE_500, CHECK_DARK, CHECK_LIGHT, FIELD_CAPTION,
        HOVER_BORDER, PINK_500, RED_500, RED_500_85, ROSE_500, SLIDER_DARK, SLIDER_MID_BRIGHT,
        SLIDER_MID_CONTRAST, SLIDER_THUMB, TEXT_55, TEXT_90, TEXT_DIM_60, TEXT_DIM_80, WHITE,
        WHITE_10, WHITE_20, WHITE_30, WHITE_45, WHITE_45_PCT, WHITE_5, WHITE_55, WHITE_80,
        YELLOW_500, ZINC_100, ZINC_300,
    };

    /// `oklch(0.205 0 0)` = `#171717` = rgb(23, 23, 23)
    pub const BG: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.0902,
        a: 1.0,
    };

    /// `oklch(0.19 0 0)` = `#141414` = rgb(20, 20, 20)
    pub const TOPBAR: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.0784,
        a: 1.0,
    };

    /// `oklch(0.235 0 0)` = `#1E1E1E` = rgb(30, 30, 30)
    pub const PANEL: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.1176,
        a: 1.0,
    };

    /// `oklch(0.17 0 0)` = `#0F0F0F` = rgb(15, 15, 15)
    pub const CANVAS: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.0588,
        a: 1.0,
    };

    /// `oklch(0.265 0 0)` = `#252525` = rgb(37, 37, 37)
    pub const FIELD: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.1451,
        a: 1.0,
    };

    /// `oklch(0.34 0 0)` = `#383838` = rgb(56, 56, 56)
    pub const FIELD_BORDER: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.2196,
        a: 1.0,
    };

    /// `oklch(0.3 0 0)` = `#2E2E2E` = rgb(46, 46, 46)
    ///
    /// This is the divider value the original theme names - but see
    /// [`BORDER`] before using it as one: it is only what reaches the screen
    /// when the thing carrying it is a **background**.
    pub const DIVIDER: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.1804,
        a: 1.0,
    };

    /// `oklch(0.32 0 0)` = `#333333` = rgb(51, 51, 51) - **the colour every
    /// border in Photrez actually paints.**
    ///
    /// The design defines a global border colour, `oklch(0.32 0 0)`, that
    /// outranks the per-element border colour a component asks for, so a
    /// divider or a white-at-20% edge still lands on #333333 in the rendered
    /// result.
    ///
    /// Only the border colour is global: fills keep their own token, so a
    /// divider *background* still paints [`DIVIDER`] (#2e2e2e) and must keep
    /// using it. Two tokens, not one, because the rule reaches one property
    /// and not the other.
    ///
    /// Verified rather than reasoned - sampling the border row/column of four
    /// independent shell baselines:
    ///
    /// | shot | rule | pixels |
    /// |---|---|---|
    /// | `shell-titlebar` | bottom border | #333333 x 1366/1366 |
    /// | `shell-option-bar` | bottom border | #333333 x 781/781 |
    /// | `shell-status-bar` | top border | #333333 x 1366/1366 |
    /// | `shell-right-dock` | left border | #333333 x 750/750 |
    ///
    /// Every one is unanimous, with no #2e2e2e anywhere on the rule - so this
    /// is not an anti-aliasing artefact and not a rounding difference.
    ///
    /// A second, independent line of evidence: the rail's swatch rings use a
    /// white border at 20% (#484848 over the #1b1b1b rail) yet the rail diff
    /// still reports 98.3% agreement with a raw diff of 2.377% - 927 pixels
    /// off. That residue is unexplained unless the override applies to those
    /// rings too. It hides from the hard count only because the classifier
    /// buckets both #333333 (luma 51) and #484848 (luma 72) as `active`.
    pub const BORDER: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.2,
        a: 1.0,
    };

    /// `oklch(0.22 0 0)` = `#1B1B1B` = rgb(27, 27, 27)
    pub const TOOLBAR: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.1059,
        a: 1.0,
    };

    /// `#1D2026` = rgb(29, 32, 38): the option bar's own overflow and popover
    /// panels. The shared dropdown list uses a different, darker fill.
    pub const BAR_POPOVER: Hsla = Hsla {
        h: 0.6111,
        s: 0.1343,
        l: 0.1314,
        a: 1.0,
    };

    /// `#1B1D22` = rgb(27, 29, 34): the dropdown and option lists. Darker than
    /// the panel it opens from, with a blue cast.
    pub const MENU_LIST: Hsla = Hsla {
        h: 0.619,
        s: 0.1148,
        l: 0.1196,
        a: 1.0,
    };

    /// `rgb(0, 44, 34)`: the deep green behind a quality or snap badge. The
    /// palette's `RED_500`/`YELLOW_500` are theme stops, not these tiers.
    pub const EMERALD_950: Hsla = Hsla {
        h: 0.4621,
        s: 1.0,
        l: 0.0863,
        a: 1.0,
    };

    /// `rgb(251, 44, 54)`: the destructive-action red, painted translucent over
    /// the panel it sits on. Distinct from `danger` (`#ef4444`), which is the
    /// theme's own mid red.
    pub const DESTRUCTIVE: Hsla = Hsla {
        h: 0.992,
        s: 0.9628,
        l: 0.5784,
        a: 1.0,
    };

    /// `#FACC15` = rgb(250, 204, 21): the busy dot in the status bar's save
    /// group. A tier of the palette, not the theme's `YELLOW_500` stop.
    pub const YELLOW_400: Hsla = Hsla {
        h: 0.13319,
        s: 0.95816,
        l: 0.53137,
        a: 1.0,
    };

    /// `#4ADE80` = rgb(74, 222, 128): the saved dot.
    pub const GREEN_400: Hsla = Hsla {
        h: 0.39414,
        s: 0.69159,
        l: 0.58039,
        a: 1.0,
    };

    /// `#F87171` = rgb(248, 113, 113): the failed-save dot, a lighter red than
    /// `RED_500`, which the design keeps for the label and the error text.
    pub const RED_400: Hsla = Hsla {
        h: 0.0,
        s: 0.90604,
        l: 0.70784,
        a: 1.0,
    };

    /// `#4C0519` = rgb(76, 5, 25): the deep rose a printer or save error is
    /// reported on. The design paints it at 20% over the field behind it.
    pub const ROSE_950: Hsla = Hsla {
        h: 0.95305,
        s: 0.87654,
        l: 0.15882,
        a: 1.0,
    };

    /// `#FB7185` = rgb(251, 113, 133): the rose text tier of an error banner.
    pub const ROSE_400: Hsla = Hsla {
        h: 0.97585,
        s: 0.94521,
        l: 0.71373,
        a: 1.0,
    };

    /// `#34D399` = rgb(52, 211, 153): the text tier of a good-quality badge, the
    /// light partner of `EMERALD_950`.
    pub const EMERALD_400: Hsla = Hsla {
        h: 0.43920,
        s: 0.64372,
        l: 0.51569,
        a: 1.0,
    };

    /// `#FBBF24` = rgb(251, 191, 36): the amber text tier of an acceptable
    /// quality badge.
    pub const AMBER_400: Hsla = Hsla {
        h: 0.12016,
        s: 0.96413,
        l: 0.56275,
        a: 1.0,
    };

    /// `#451A03` = rgb(69, 26, 3): the amber badge fill, painted at 40% over the
    /// panel behind it, like `EMERALD_950` and `ROSE_950`.
    pub const AMBER_950: Hsla = Hsla {
        h: 0.05808,
        s: 0.91667,
        l: 0.14118,
        a: 1.0,
    };

    /// `#E15A17` = rgb(225, 90, 23): the tool mark, a darker orange than
    /// `ACCENT` (`#F28E42`). It only appears where the layout draws through an
    /// SVG stroke attribute or a cursor asset, which no CSS rule can override,
    /// so the two must not be merged.
    pub const TOOL_MARK: Hsla = Hsla {
        h: 0.05528,
        s: 0.81452,
        l: 0.48627,
        a: 1.0,
    };

    /// `oklch(0.3 0 0)` = `#2E2E2E` = rgb(46, 46, 46)
    pub const ROW_ACTIVE: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.1804,
        a: 1.0,
    };

    /// `oklch(0.255 0 0)` = `#232323` = rgb(35, 35, 35)
    pub const TAB_ACTIVE: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.1373,
        a: 1.0,
    };

    /// `oklch(0.84 0 0)` = `#CACACA` = rgb(202, 202, 202)
    pub const TEXT: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.7922,
        a: 1.0,
    };

    /// `oklch(0.58 0 0)` = `#7A7A7A` in the source palette; raised to `#8C8C8C`
    /// = rgb(140, 140, 140) for legibility.
    ///
    /// At 122 the dim text measured 3.66:1 to 4.18:1 against the shell's dark
    /// surfaces, below the 4.5:1 AA floor for body text. 140 clears it on every
    /// surface this colour lands on (worst pairing 4.67:1, on `TAB_ACTIVE`).
    /// Pinned by `core_values_are_pinned` and guarded by
    /// `text_dim_meets_aa_on_dark_surfaces`.
    pub const TEXT_DIM: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.5490,
        a: 1.0,
    };

    /// `oklch(0.62 0 0)` = `#868686` = rgb(134, 134, 134)
    pub const ICON: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.5255,
        a: 1.0,
    };

    /// `oklch(0.74 0.15 55)` = `#F28E42` = rgb(242, 142, 66)
    pub const ACCENT: Hsla = Hsla {
        h: 0.072,
        s: 0.8713,
        l: 0.6039,
        a: 1.0,
    };

    /// `oklch(0.62 0.2 36)` = `#E44919` = rgb(228, 73, 25)
    pub const BRAND: Hsla = Hsla {
        h: 0.0394,
        s: 0.8024,
        l: 0.4961,
        a: 1.0,
    };
}

/// Static dimensions for the title bar and the menu bar, in pixels.
///
/// Kept separate from [`dim`] because both surfaces define their own
/// `MENU_TEXT`, and the two are different sizes (11px in the tool fly-out,
/// 12px in the application menu).
pub mod title {
    /// 46px tall for the header row. Its box includes its own border, so the
    /// 1px bottom edge is taken out of this, leaving 45px of content - which is
    /// why the 46px window-control buttons overflow by 0.5px top and bottom and
    /// are clipped to the bar.
    pub const HEADER_H: f32 = 46.0;
    /// 12px left inset on the logo cell.
    pub const LOGO_PAD_L: f32 = 12.0;
    /// Left inset for the logo cell **on macOS only**, replacing `LOGO_PAD_L`.
    ///
    /// macOS keeps AppKit's traffic lights pinned to the top-left of a titled
    /// window even when the title bar is transparent - `NSTitledWindowMask` is
    /// never dropped, only `titlebarAppearsTransparent` and a hidden title are
    /// set. The cluster starts at the top-left and runs about three 20px
    /// buttons wide.
    ///
    /// Photrez's logo occupies x = 12..42, which is exactly that space, so on
    /// macOS the logo is pushed clear of the traffic lights and the menu bar
    /// follows it - the standard arrangement for a Mac app that keeps its mark
    /// beside the window buttons.
    ///
    /// **Estimate, unverified**: this machine is Windows, so the clearance is
    /// derived from AppKit's documented button pitch rather than measured.
    /// Read it back on a Mac before trusting it; a couple of pixels either way
    /// only affects where the logo starts, not the rest of the bar.
    pub const LOGO_PAD_L_MACOS: f32 = 78.0;
    /// 8px right inset on the logo cell.
    pub const LOGO_PAD_R: f32 = 8.0;
    /// 30px square for the logo graphic.
    pub const LOGO: f32 = 30.0;
    /// 8px between the logo cell and the menu bar. Together with the logo
    /// cell (12 + 30 + 8 = 50) this puts the first menu trigger at x = 58,
    /// which is what the baseline measured.
    pub const LEFT_GAP: f32 = 8.0;
    /// 2px between menu triggers.
    pub const MENU_GAP: f32 = 2.0;
    /// 26px tall for a menu trigger.
    pub const MENU_H: f32 = 26.0;
    /// 4px corner radius - menu triggers and the undo/redo buttons.
    pub const RADIUS: f32 = 4.0;
    /// 10px horizontal padding on a menu trigger, before the tracking
    /// compensation below.
    pub const MENU_PX: f32 = 10.0;
    /// 12px text - menu triggers and the document name.
    pub const MENU_TEXT: f32 = 12.0;
    /// Wide letter-spacing: `0.025em`, which at [`MENU_TEXT`] is 0.3px per
    /// glyph.
    ///
    /// GPUI's text system has no letter-spacing at all; the title bar
    /// compensates for it in measurement.
    pub const TRACKING_EM: f32 = 0.025;
    /// 16px minimum width on the centre cell.
    pub const CENTER_MIN_W: f32 = 16.0;
    /// 6px inside the centre cell.
    pub const CENTER_GAP: f32 = 6.0;
    /// 11px on the `.` separator.
    pub const SEP_TEXT: f32 = 11.0;
    /// 11.5px on the "photrez" wordmark.
    pub const WORDMARK_TEXT: f32 = 11.5;
    /// 12px right margin on the undo/redo group.
    pub const UNDO_MR: f32 = 12.0;
    /// 2px inside the undo/redo group.
    pub const UNDO_GAP: f32 = 2.0;
    /// 28px square on the undo/redo buttons.
    pub const UNDO_BTN: f32 = 28.0;
    /// 4px horizontal margin on the vertical rule after the redo button.
    pub const RULE_MX: f32 = 4.0;
    /// 14px tall for that rule.
    pub const RULE_H: f32 = 14.0;
    /// 44px wide for a window-control button.
    pub const WINDOW_BTN_W: f32 = 44.0;
    /// 46px tall for a window-control button - deliberately the *full* header
    /// height, not the 45px content box.
    pub const WINDOW_BTN_H: f32 = 46.0;
    /// 16px square - undo, redo and close icons.
    pub const ICON_16: f32 = 16.0;
    /// 15px square - the minimize icon.
    pub const ICON_15: f32 = 15.0;
    /// 12px square - the maximize icon.
    pub const ICON_12: f32 = 12.0;
    /// A 1.75 stroke width on every icon in this component.
    pub const ICON_STROKE: f32 = 1.75;
}

/// Static dimensions for the tool rail, in pixels.
///
/// The rem scale assumes a 16px root, so every `_1.._N` step below is
/// already resolved: 0.125rem = 2px, 0.5rem = 8px, etc.
pub mod dim {
    /// 52px - rail width.
    pub const RAIL_W: f32 = 52.0;
    /// 8px (0.5rem) vertical padding.
    pub const RAIL_PY: f32 = 8.0;
    /// 2px (0.125rem).
    pub const RAIL_GAP: f32 = 2.0;
    /// 36px square - tool button.
    pub const TOOL_BTN: f32 = 36.0;
    /// 5px tool-button corner. Deliberately *not* the theme's 0.375rem radius
    /// scale; this component hardcodes 5px.
    pub const TOOL_BTN_RADIUS: f32 = 5.0;
    /// 18px square - tool button icon.
    pub const ICON: f32 = 18.0;
    /// A 1.6 stroke width passed as `strokeWidth` to every icon.
    /// lucide's default is 2.
    pub const ICON_STROKE: f32 = 1.6;
    /// 1px - divider height.
    pub const DIVIDER_H: f32 = 1.0;
    /// 24px (1.5rem) - divider width.
    pub const DIVIDER_W: f32 = 24.0;
    /// 4px (0.25rem) - space below the divider.
    pub const DIVIDER_MB: f32 = 4.0;
    /// 38px square - swatch stack container.
    pub const SWATCH_BOX: f32 = 38.0;
    /// 8px (0.5rem) - space above/below the swatch stack.
    pub const SWATCH_MY: f32 = 8.0;
    /// 28px square - each overlapping colour circle.
    pub const SWATCH: f32 = 28.0;
    /// -6px (-0.375rem) up and right - swap button offset.
    pub const SWAP_OFFSET: f32 = -6.0;
    /// 16px (1rem) - swap button.
    pub const SWAP_BTN: f32 = 16.0;
    /// 10px (0.625rem) - swap button icon.
    pub const SWAP_ICON: f32 = 10.0;
    /// 4px (0.25rem) from the bottom right - sub-variant triangle inset.
    pub const TRIANGLE_INSET: f32 = 4.0;
    /// 5px - sub-variant triangle.
    pub const TRIANGLE: f32 = 5.0;
    /// 11px - fly-out menu row font size.
    pub const MENU_TEXT: f32 = 11.0;
    /// 16px (1rem) - fly-out menu row icon.
    pub const MENU_ICON: f32 = 16.0;
}

/// Test-only HSL to 8-bit RGB. The palette stores HSL, so this reproduces the
/// pixel a capture compares.
#[cfg(test)]
pub(crate) fn hsla_to_rgb(colour: gpui_kit::Hsla) -> (u8, u8, u8) {
    fn sector(p: f32, q: f32, mut t: f32) -> f32 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 0.5 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    }
    let gpui_kit::Hsla { h, s, l, a: _ } = colour;
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    (
        (sector(p, q, h + 1.0 / 3.0) * 255.0).round() as u8,
        (sector(p, q, h) * 255.0).round() as u8,
        (sector(p, q, h - 1.0 / 3.0) * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::{color, dim, hsla_to_rgb, title};

    /// The bar's own popovers paint `#1D2026`, one step above the bar itself and
    /// a different fill again from the shared dropdown list.
    #[test]
    fn bar_popover_paints_the_reference_popover_fill() {
        assert_eq!(hsla_to_rgb(color::BAR_POPOVER), (29, 32, 38));
        assert_eq!(hsla_to_rgb(color::TOOLBAR), (27, 27, 27));
    }

    /// The lists sit a step below the panel they open from, so the neutral panel
    /// colour is not an acceptable substitute.
    #[test]
    fn menu_list_paints_the_reference_dropdown_fill() {
        assert_eq!(hsla_to_rgb(color::MENU_LIST), (27, 29, 34));
        assert_eq!(hsla_to_rgb(color::PANEL), (30, 30, 30));
    }

    /// The tool mark is a different orange from the theme accent, and the layout
    /// uses each in a different construction, so they must not merge.
    #[test]
    fn tool_mark_is_not_the_theme_accent() {
        assert_eq!(hsla_to_rgb(color::TOOL_MARK), (225, 90, 23));
        assert_eq!(hsla_to_rgb(color::ACCENT), (242, 142, 66));
    }

    /// The save-state dots paint at full opacity, so the tier itself is the
    /// pixel a capture compares.
    #[test]
    fn save_state_dots_paint_their_palette_tiers() {
        assert_eq!(hsla_to_rgb(color::YELLOW_400), (250, 204, 21));
        assert_eq!(hsla_to_rgb(color::GREEN_400), (74, 222, 128));
        assert_eq!(hsla_to_rgb(color::RED_400), (248, 113, 113));
    }

    /// The printer-error banner paints rose at 20% over the field behind it, so
    /// the composited pixel is what a capture sees.
    #[test]
    fn error_banner_tier_composites_over_the_field() {
        assert_eq!(hsla_to_rgb(color::ROSE_400), (251, 113, 133));
        let field = hsla_to_rgb(color::FIELD);
        let rose = hsla_to_rgb(color::ROSE_950);
        let a = 0.2_f32;
        let mix = |t: u8, base: u8| (t as f32 * a + base as f32 * (1.0 - a)).round() as u8;
        assert_eq!(field, (37, 37, 37));
        assert_eq!(
            (
                mix(rose.0, field.0),
                mix(rose.1, field.1),
                mix(rose.2, field.2)
            ),
            (45, 31, 35)
        );
    }

    /// The badge and destructive tiers are painted translucent over the panel, so
    /// the composited pixel is what a capture compares, not the tier alone.
    #[test]
    fn badge_and_destructive_tiers_composite_over_the_panel() {
        let panel = hsla_to_rgb(color::PANEL);
        let over = |tier: (u8, u8, u8), a: f32| {
            let mix = |t: u8, base: u8| (t as f32 * a + base as f32 * (1.0 - a)).round() as u8;
            (
                mix(tier.0, panel.0),
                mix(tier.1, panel.1),
                mix(tier.2, panel.2),
            )
        };
        assert_eq!(hsla_to_rgb(color::EMERALD_950), (0, 44, 34));
        assert_eq!(over((0, 44, 34), 0.4), (18, 36, 32));
        assert_eq!(hsla_to_rgb(color::DESTRUCTIVE), (251, 44, 54));
        assert_eq!(over((251, 44, 54), 0.1), (52, 31, 32));
    }

    /// ACCENT (`oklch(0.74 0.15 55)`) and BRAND (`oklch(0.62 0.2 36)`) carry
    /// chroma that sits marginally outside sRGB, so the previous renderer clips
    /// them before rendering. The tokens are the *clipped* result, not the raw
    /// OKLCH value - so their h/s/l must reproduce the exact sRGB the previous
    /// renderer lands on. These pins assert that holds. A recompute that skips the
    /// clip (or re-derives from OKLCH without clamping) will change the hue,
    /// saturation or lightness and break the assertion.
    #[test]
    fn accent_and_brand_are_clipped_not_raw_oklch() {
        // Clipped sRGB: oklch(0.74 0.15 55) -> #F28E42 -> HSL.
        assert_eq!(color::ACCENT.h, 0.072);
        assert_eq!(color::ACCENT.s, 0.8713);
        assert_eq!(color::ACCENT.l, 0.6039);

        // Clipped sRGB: oklch(0.62 0.2 36) -> #E44919 -> HSL.
        assert_eq!(color::BRAND.h, 0.0394);
        assert_eq!(color::BRAND.s, 0.8024);
        assert_eq!(color::BRAND.l, 0.4961);
    }

    /// A silent edit to the table (e.g. one colour's lightness drifting, or a
    /// shell geometry constant re-measured) must redden. Pins a few exact
    /// colours and the measured shell geometry numbers.
    #[test]
    fn core_values_are_pinned() {
        // Exact chromatic + neutral colours, straight from the theme block.
        assert_eq!(color::BG.l, 0.0902);
        assert_eq!(color::PANEL.l, 0.1176);
        assert_eq!(color::CANVAS.l, 0.0588);
        assert_eq!(color::TEXT.l, 0.7922);
        // Raised from the source's 0.4784 (#7A7A7A) for AA contrast.
        assert_eq!(color::TEXT_DIM.l, 0.5490);
        assert_eq!(color::ICON.l, 0.5255);
        assert_eq!(color::DIVIDER.l, 0.1804);
        // The global border override that every border actually paints.
        assert_eq!(color::BORDER.l, 0.2);
        assert_eq!(color::WHITE.l, 1.0);
        // The crop bar's faded Apply: ACCENT at 15% (the resting capture
        // blends to 59/44/33 over the 27 bar; the pixel pen reads 58/43/32).
        assert_eq!(color::ACCENT_15.h, 0.072);
        assert_eq!(color::ACCENT_15.s, 0.8713);
        assert_eq!(color::ACCENT_15.l, 0.6039);
        assert_eq!(color::ACCENT_15.a, 0.15);

        // Measured shell geometry.
        assert_eq!(title::HEADER_H, 46.0);
        assert_eq!(dim::RAIL_W, 52.0);
        assert_eq!(dim::TOOL_BTN, 36.0);
        assert_eq!(dim::SWATCH, 28.0);
        assert_eq!(dim::RAIL_PY, 8.0);
    }

    /// Dim text must clear WCAG AA (4.5:1) on every dark surface it lands on.
    /// Reverting the lightness toward the source's 0.4784 must redden this.
    #[test]
    fn text_dim_meets_aa_on_dark_surfaces() {
        // HSL greys only: h and s are 0, so the single channel is the RGB
        // value the WCAG transfer function needs. Non-grey tokens would need
        // per-channel math and do not appear here.
        fn luminance(channel: f32) -> f32 {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        }
        fn ratio(a: f32, b: f32) -> f32 {
            let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
            (hi + 0.05) / (lo + 0.05)
        }

        let text = luminance(color::TEXT_DIM.l);
        for (name, surface) in [
            ("PANEL", color::PANEL.l),
            ("TOOLBAR", color::TOOLBAR.l),
            ("TAB_ACTIVE", color::TAB_ACTIVE.l),
            ("BG", color::BG.l),
        ] {
            let measured = ratio(text, luminance(surface));
            assert!(
                measured >= 4.5,
                "TEXT_DIM on {name} measures {measured:.2}:1, below AA 4.5:1"
            );
        }
    }
}
