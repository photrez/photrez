#![allow(dead_code)] // consumers land module-by-module; drop this blanket as they arrive
//! Design tokens for the gpui-kit port of Photrez.
//!
//! Source of truth: the `@theme` block in `apps/desktop/src/styles.css`.
//! The (stale) `docs/reference/design-tokens.md` disagrees with the shipped
//! code in places - e.g. it describes a 35px diagonal clip-path swatch, while
//! `LeftToolRail.tsx` actually renders two overlapping 28px circles. This file
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
//! chroma 0.15/0.20 at those hues sits marginally outside sRGB, so the browser
//! clips them too. We clip identically, so the rendered colour matches.
//!
//! # Known fidelity gaps vs. the Tailwind original
//!
//! * OKLCH is not representable, so the ramp is only as smooth as sRGB allows.
//!   For this palette chroma is 0 for every neutral, so the gap is zero.
//! * `--radius-*` in styles.css derives from `calc(var(--radius) - Npx)` and is
//!   never used by `LeftToolRail`, which hardcodes `rounded-[5px]`.

/// Design tokens from the `@theme` block of `apps/desktop/src/styles.css`,
/// keyed by the `--editor-*` variable names.
///
/// Import as `gpui_kit::Hsla` rather than `gpui::Hsla`: this crate depends only
/// on `gpui-kit`, which re-exports GPUI at its root (`pub use ::gpui::*`) but
/// does not make a `gpui` crate available to dependents.
pub mod color {
    use gpui_kit::Hsla;

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
    /// This is `--editor-divider`, the value the class *names* - but see
    /// [`BORDER`] before using it as one: it is only what reaches the screen
    /// when the thing carrying it is a **background**.
    pub const DIVIDER: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.1804,
        a: 1.0,
    };

    /// `oklch(0.32 0 0)` = `#333333` = rgb(51, 51, 51) - **the colour every
    /// `border-*` in Photrez actually paints.**
    ///
    /// `styles.css` declares, outside any cascade layer:
    ///
    /// ```css
    /// * { box-sizing: border-box; border-color: var(--color-border); }
    /// ```
    ///
    /// with `--color-border: var(--border)` = `oklch(0.32 0 0)`. Tailwind v4's
    /// utilities live in `@layer utilities` (via `@import "tailwindcss"`), and
    /// CSS Cascade 5 sorts unlayered author declarations into an implicit final
    /// layer that outranks every declared layer - **regardless of
    /// specificity**. So this `0,0,0` universal selector beats
    /// `.border-editor-divider`, `.border-white/20`, and every other
    /// border-colour utility the design asks for.
    ///
    /// The rule sets only `border-color`, so `background-*` is untouched:
    /// `bg-editor-divider` still paints [`DIVIDER`] (#2e2e2e) and must keep
    /// using it. Two tokens, not one, because the cascade reaches one property
    /// and not the other.
    ///
    /// Verified rather than reasoned - sampling the border row/column of four
    /// independent shell baselines:
    ///
    /// | shot | rule | pixels |
    /// |---|---|---|
    /// | `shell-titlebar` | `border-b` | #333333 x 1366/1366 |
    /// | `shell-option-bar` | `border-b` | #333333 x 781/781 |
    /// | `shell-status-bar` | `border-t` | #333333 x 1366/1366 |
    /// | `shell-right-dock` | `border-l` | #333333 x 750/750 |
    ///
    /// Every one is unanimous, with no #2e2e2e anywhere on the rule - so this
    /// is not an anti-aliasing artefact and not a rounding difference.
    ///
    /// A second, independent line of evidence: the rail's swatch rings use
    /// `border-white/20` (#484848 over the #1b1b1b rail) yet the rail diff
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

    // --- Alpha-modified tokens (Tailwind `/N` syntax) ---

    /// `bg-white/5` on tool buttons + hover state on inactive buttons.
    pub const WHITE_5: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 0.05,
    };

    /// `bg-white/10` on the active row in the variant fly-out.
    pub const WHITE_10: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 0.10,
    };

    /// `border-white/20` on the background swatch ring.
    pub const WHITE_20: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 0.20,
    };

    /// `border-white/30` on the foreground swatch ring.
    pub const WHITE_30: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 0.30,
    };

    /// `outline-black/40` on the foreground swatch.
    pub const BLACK_40: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.0,
        a: 0.40,
    };

    /// `text-editor-text-dim/80` - the sub-variant triangle indicator.
    pub const TEXT_DIM_80: Hsla = Hsla {
        h: TEXT_DIM.h,
        s: TEXT_DIM.s,
        l: TEXT_DIM.l,
        a: 0.8,
    };

    // --- AppTitleBar / AppMenuBar ---

    /// `text-editor-text/90` - a menu trigger at rest.
    pub const TEXT_90: Hsla = Hsla {
        h: TEXT.h,
        s: TEXT.s,
        l: TEXT.l,
        a: 0.9,
    };

    /// `text-editor-text-dim/60` - the `.` between the document name and the
    /// "photrez" wordmark.
    pub const TEXT_DIM_60: Hsla = Hsla {
        h: TEXT_DIM.h,
        s: TEXT_DIM.s,
        l: TEXT_DIM.l,
        a: 0.6,
    };

    /// `hover:bg-white/[0.045]` - the shared *bracket-form* hover: menu
    /// triggers, undo/redo, the document tab's close button and the two
    /// header icon buttons (`DocumentTabsBar.tsx:80,109,128`).
    ///
    /// Four and a half percent, not forty-five. See [`WHITE_45_PCT`] for the
    /// other value and why their names are so easy to swap.
    pub const WHITE_45: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 0.045,
    };

    /// `hover:bg-white/[0.055]` - the minimize and maximize buttons.
    pub const WHITE_55: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 0.055,
    };

    /// `bg-white/[0.08]` - the menu trigger whose popup is open.
    pub const WHITE_80: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 0.08,
    };

    /// `hover:bg-red-500/85` - the close-window button.
    ///
    /// Tailwind v4's `red-500` is `oklch(63.7% 0.237 25.331)` = `#FB2C36` =
    /// rgb(251, 44, 54). Converted to HSL: max 251, min 44, delta 207,
    /// hue 60 * ((44 - 54) / 207) = -2.90deg = 357.10deg, l = 147.5 / 255,
    /// s = delta / (2 - max - min) = 0.8118 / 0.8431.
    ///
    /// Only reachable on hover, which the title-bar baseline does not capture;
    /// it is specified here for completeness rather than for the diff.
    pub const RED_500_85: Hsla = Hsla {
        h: 0.991948,
        s: 0.962799,
        l: 0.578431,
        a: 0.85,
    };

    /// `bg-editor-accent/15` - the crop bar's `Apply` interior in its resting
    /// capture: `#3a2b20` (58, 43, 32) over the `#1b1b1b` bar, which is
    /// `ACCENT` at 15% (the blend lands on 59, 44, 33). The transform bar's
    /// full-strength `Apply` variant is not in any baseline, so this is pinned
    /// from `option-crop.png`.
    pub const ACCENT_15: Hsla = Hsla {
        h: 0.072,
        s: 0.8713,
        l: 0.6039,
        a: 0.15,
    };

    // -- Ad-hoc literals, not `--editor-*` tokens ----------------------------
    //
    // Everything above this line is a `--editor-*` / Tailwind theme variable:
    // one value, one meaning, reused across the app. The colours below are the
    // other kind - hex literals written straight into a component's class
    // string, so they have no variable to look up and are named here by the
    // *part* they colour instead.
    //
    // All eleven come from `apps/desktop/src/components/editor/primitives.tsx`
    // (the five shared controls), which is the module that introduces them and
    // the reason they are worth sharing: `PropertiesPanel`,
    // `AdjustmentsPanel` and the option bars all build on those controls, so a
    // value pinned here is pinned once rather than transcribed per panel.
    //
    // Provenance is the literal class, quoted; the `--editor-*` values above
    // were derived by the OKLCH -> sRGB -> HSL path, but these bypass it -
    // there is no `oklch()` to expand, only a hex to turn into HSL.

    /// `text-[#A1A1AA]` - the caption colour on a field: its `label` and its
    /// `suffix` (`primitives.tsx:23,32,59,61,271,321`).
    ///
    /// Not `color::TEXT_DIM` (#8c8c8c) and not `color::ICON` (#868686): this
    /// one is a cool grey with a blue cast (hue 240deg, chroma 0.050), which is
    /// what separates a field caption from the surrounding chrome.
    pub const FIELD_CAPTION: Hsla = Hsla {
        h: 0.666667,
        s: 0.050296,
        l: 0.649020,
        a: 1.0,
    };

    /// `hover:border-[#4B515D]` - the border a field moves to on hover
    /// (`primitives.tsx:42,263`).
    ///
    /// Only `SelectField` and `EditableNumField` ask for it; `NumField` has no
    /// hover state at all, which is a real difference between the two and not
    /// an omission.
    pub const HOVER_BORDER: Hsla = Hsla {
        h: 0.611111,
        s: 0.107071,
        l: 0.329412,
        a: 1.0,
    };

    /// `bg-[#d8dce2]` - the `Slider` thumb (`primitives.tsx:198`).
    pub const SLIDER_THUMB: Hsla = Hsla {
        h: 0.600000,
        s: 0.147059,
        l: 0.866667,
        a: 1.0,
    };

    /// `#09090b` - the dark end of the `brightness` and `contrast` track
    /// gradients (`primitives.tsx:115,120`).
    pub const SLIDER_DARK: Hsla = Hsla {
        h: 0.666667,
        s: 0.100000,
        l: 0.039216,
        a: 1.0,
    };

    /// `#52525b` - `brightness`'s midpoint, and `saturation`'s start
    /// (`primitives.tsx:115,125`).
    pub const SLIDER_MID_BRIGHT: Hsla = Hsla {
        h: 0.666667,
        s: 0.052019,
        l: 0.339216,
        a: 1.0,
    };

    /// `#71717a` - `contrast`'s midpoint, and `saturation`'s 35% stop
    /// (`primitives.tsx:120,125`).
    pub const SLIDER_MID_CONTRAST: Hsla = Hsla {
        h: 0.666667,
        s: 0.038331,
        l: 0.460784,
        a: 1.0,
    };

    /// `#2d3139` - the light square of the transparency checkerboard that sits
    /// under the `opacity` / `brush-strength` track (`primitives.tsx:109`).
    ///
    /// Paired with [`CHECK_DARK`]. Both are written into a `conic-gradient`
    /// rather than a `background-color`, so a solid fill cannot substitute: the
    /// alternation is the point, since a slider that fades to `transparent` has
    /// to show *something* through it.
    pub const CHECK_LIGHT: Hsla = Hsla {
        h: 0.611111,
        s: 0.117647,
        l: 0.200000,
        a: 1.0,
    };

    /// `#1a1d24` - the dark square of that checkerboard (`primitives.tsx:109`).
    pub const CHECK_DARK: Hsla = Hsla {
        h: 0.616667,
        s: 0.161290,
        l: 0.121569,
        a: 1.0,
    };

    /// `#ef4444` - `saturation`'s 60% stop (`primitives.tsx:125`); Tailwind's
    /// `red-500`.
    pub const RED_500: Hsla = Hsla {
        h: 0.0,
        s: 0.842424,
        l: 0.601961,
        a: 1.0,
    };

    /// `#eab308` - `saturation`'s 80% stop (`primitives.tsx:125`); Tailwind's
    /// `yellow-500`.
    pub const YELLOW_500: Hsla = Hsla {
        h: 0.126064,
        s: 0.933941,
        l: 0.474510,
        a: 1.0,
    };

    /// `#3b82f6` - `saturation`'s 100% stop (`primitives.tsx:125`); Tailwind's
    /// `blue-500`.
    pub const BLUE_500: Hsla = Hsla {
        h: 0.603354,
        s: 0.912216,
        l: 0.598039,
        a: 1.0,
    };

    /// `bg-zinc-100` - the light end of the `brightness` fill
    /// (`primitives.tsx:167`).
    pub const ZINC_100: Hsla = Hsla {
        h: 0.666667,
        s: 0.047619,
        l: 0.958824,
        a: 1.0,
    };

    /// `bg-zinc-300` - the light end of the `contrast` fill
    /// (`primitives.tsx:168`).
    pub const ZINC_300: Hsla = Hsla {
        h: 0.666667,
        s: 0.048780,
        l: 0.839216,
        a: 1.0,
    };

    /// `from-pink-500` - the start of the `saturation` fill
    /// (`primitives.tsx:169`), paired with [`ROSE_500`].
    pub const PINK_500: Hsla = Hsla {
        h: 0.917683,
        s: 0.811881,
        l: 0.603922,
        a: 1.0,
    };

    /// `to-rose-500` - the end of the `saturation` fill (`primitives.tsx:169`).
    pub const ROSE_500: Hsla = Hsla {
        h: 0.971455,
        s: 0.891626,
        l: 0.601961,
        a: 1.0,
    };

    /// `bg-editor-text/55` - the `Slider` fill when `accent={false}`
    /// (`primitives.tsx:189`): [`TEXT`] at 55% alpha.
    pub const TEXT_55: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.792157,
        a: 0.55,
    };

    /// `bg-white/45` - the `Slider`'s centre tick (`primitives.tsx:175`).
    ///
    /// **Not** [`WHITE_45`], despite the identical-looking name. That one is
    /// Tailwind's *bracket* form, `bg-white/[0.045]`, whose suffix and this
    /// one's slash value both read as "45" - so `WHITE_45` is four and a half
    /// percent and this is forty-five. The suffix tracks the source's
    /// spelling rather than a unit, which is what makes the collision
    /// possible; the `_PCT` is here so the next reader does not assume the
    /// two are the same number written twice.
    pub const WHITE_45_PCT: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 0.45,
    };

    /// `border-black/55` - the `Slider` thumb's own 1px border
    /// (`primitives.tsx:198`).
    ///
    /// Distinct from [`BLACK_40`], which is the token this palette uses for
    /// recessed shadows; the thumb is the one place asking for 55.
    pub const BLACK_55: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.0,
        a: 0.55,
    };

    /// `text-white` - Tailwind's unmodified white, `#ffffff`.
    ///
    /// The one value here that is not really a colour choice: it is the
    /// absence of one, which is why the palette never gave it a variable.
    /// It is named because it is everywhere (`primitives.tsx:30,46,189,318`)
    /// and each of its uses is a *foreground* that must not drift with the
    /// chrome behind it - a field's value, a selected option, a label that
    /// only appears once hovered. Spelling it as `l: 1.0` at every call site
    /// would read as an arbitrary grey to the next reader.
    pub const WHITE: Hsla = Hsla {
        h: 0.0,
        s: 0.0,
        l: 1.0,
        a: 1.0,
    };
}

/// Static dimensions from `AppTitleBar.tsx` and `AppMenuBar.tsx`, in pixels.
///
/// Kept separate from [`dim`] because both components define their own
/// `MENU_TEXT`, and the two are different sizes (11px in the tool fly-out,
/// 12px in the application menu).
pub mod title {
    /// `h-[46px]` on `<header>`. Box-sizing is border-box, so the 1px
    /// `border-b` is taken out of this, leaving 45px of content - which is why
    /// the 46px window-control buttons overflow by 0.5px top and bottom and are
    /// clipped by `overflow-hidden`.
    pub const HEADER_H: f32 = 46.0;
    /// `pl-3` on the logo cell.
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
    /// `pr-2` on the logo cell.
    pub const LOGO_PAD_R: f32 = 8.0;
    /// `size-[30px]` on the logo `<svg>`.
    pub const LOGO: f32 = 30.0;
    /// `gap-2` between the logo cell and the menu bar. Together with the logo
    /// cell (12 + 30 + 8 = 50) this puts the first menu trigger at x = 58,
    /// which is what the baseline measured.
    pub const LEFT_GAP: f32 = 8.0;
    /// `gap-0.5` between menu triggers.
    pub const MENU_GAP: f32 = 2.0;
    /// `h-[26px]` on a menu trigger.
    pub const MENU_H: f32 = 26.0;
    /// `rounded-[4px]` - menu triggers and the undo/redo buttons.
    pub const RADIUS: f32 = 4.0;
    /// `px-2.5` on a menu trigger, before the tracking compensation below.
    pub const MENU_PX: f32 = 10.0;
    /// `text-[12px]` - menu triggers and the document name.
    pub const MENU_TEXT: f32 = 12.0;
    /// `tracking-wide` = `0.025em`, which at [`MENU_TEXT`] is 0.3px per glyph.
    ///
    /// GPUI's text system has no letter-spacing at all; the title bar
    /// compensates for it in measurement.
    pub const TRACKING_EM: f32 = 0.025;
    /// `min-w-4` on the centre cell.
    pub const CENTER_MIN_W: f32 = 16.0;
    /// `gap-1.5` inside the centre cell.
    pub const CENTER_GAP: f32 = 6.0;
    /// `text-[11px]` on the `.` separator.
    pub const SEP_TEXT: f32 = 11.0;
    /// `text-[11.5px]` on the "photrez" wordmark.
    pub const WORDMARK_TEXT: f32 = 11.5;
    /// `mr-3` on the undo/redo group.
    pub const UNDO_MR: f32 = 12.0;
    /// `gap-0.5` inside the undo/redo group.
    pub const UNDO_GAP: f32 = 2.0;
    /// `size-7` on the undo/redo buttons.
    pub const UNDO_BTN: f32 = 28.0;
    /// `mx-1` on the vertical rule after the redo button.
    pub const RULE_MX: f32 = 4.0;
    /// `h-3.5` on that rule.
    pub const RULE_H: f32 = 14.0;
    /// `w-11` on a window-control button.
    pub const WINDOW_BTN_W: f32 = 44.0;
    /// `h-[46px]` on a window-control button - deliberately the *full* header
    /// height, not the 45px content box.
    pub const WINDOW_BTN_H: f32 = 46.0;
    /// `size-[16px]` - undo, redo and close icons.
    pub const ICON_16: f32 = 16.0;
    /// `size-[15px]` - the minimize icon.
    pub const ICON_15: f32 = 15.0;
    /// `size-[12px]` - the maximize icon.
    pub const ICON_12: f32 = 12.0;
    /// `strokeWidth={1.75}` on every `<Icon />` in this component.
    pub const ICON_STROKE: f32 = 1.75;
}

/// Static dimensions from `LeftToolRail.tsx`, in pixels.
///
/// Tailwind's rem scale assumes a 16px root, so every `_1.._N` step below is
/// already resolved: `gap-0.5` = 0.125rem = 2px, `py-2` = 0.5rem = 8px, etc.
pub mod dim {
    /// `w-[52px]` - rail width.
    pub const RAIL_W: f32 = 52.0;
    /// `py-2` = 0.5rem.
    pub const RAIL_PY: f32 = 8.0;
    /// `gap-0.5` = 0.125rem.
    pub const RAIL_GAP: f32 = 2.0;
    /// `size-9` - tool button.
    pub const TOOL_BTN: f32 = 36.0;
    /// `rounded-[5px]` - tool button. Deliberately *not* the `--radius: 0.375rem`
    /// scale from styles.css; this component hardcodes 5px.
    pub const TOOL_BTN_RADIUS: f32 = 5.0;
    /// `size-[18px]` - tool button icon.
    pub const ICON: f32 = 18.0;
    /// `stroke-width: 1.6` passed as `strokeWidth` to every `<Icon />`.
    /// lucide's default is 2.
    pub const ICON_STROKE: f32 = 1.6;
    /// `h-px` - divider height.
    pub const DIVIDER_H: f32 = 1.0;
    /// `w-6` = 1.5rem - divider width.
    pub const DIVIDER_W: f32 = 24.0;
    /// `mb-1` = 0.25rem - space below the divider.
    pub const DIVIDER_MB: f32 = 4.0;
    /// `size-[38px]` - swatch stack container.
    pub const SWATCH_BOX: f32 = 38.0;
    /// `my-2` = 0.5rem - space above/below the swatch stack.
    pub const SWATCH_MY: f32 = 8.0;
    /// `size-[28px]` - each overlapping colour circle.
    pub const SWATCH: f32 = 28.0;
    /// `-top-1.5` / `-right-1.5` = -0.375rem - swap button offset.
    pub const SWAP_OFFSET: f32 = -6.0;
    /// `size-4` = 1rem - swap button.
    pub const SWAP_BTN: f32 = 16.0;
    /// `size-2.5` = 0.625rem - swap button icon.
    pub const SWAP_ICON: f32 = 10.0;
    /// `bottom-1 right-1` = 0.25rem - sub-variant triangle inset.
    pub const TRIANGLE_INSET: f32 = 4.0;
    /// `size-[5px]` - sub-variant triangle.
    pub const TRIANGLE: f32 = 5.0;
    /// `text-[11px]` - fly-out menu row font size.
    pub const MENU_TEXT: f32 = 11.0;
    /// `size-4` = 1rem - fly-out menu row icon.
    pub const MENU_ICON: f32 = 16.0;
}

#[cfg(test)]
mod tests {
    use super::{color, dim, title};

    /// ACCENT (`oklch(0.74 0.15 55)`) and BRAND (`oklch(0.62 0.2 36)`) carry
    /// chroma that sits marginally outside sRGB, so the browser clips them
    /// before rendering. The tokens are the *clipped* result, not the raw
    /// OKLCH value - so their h/s/l must reproduce the exact sRGB the browser
    /// lands on. These pins assert that holds. A recompute that skips the
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
        // Exact chromatic + neutral colours, straight from the @theme block.
        assert_eq!(color::BG.l, 0.0902);
        assert_eq!(color::PANEL.l, 0.1176);
        assert_eq!(color::CANVAS.l, 0.0588);
        assert_eq!(color::TEXT.l, 0.7922);
        // Raised from the source's 0.4784 (#7A7A7A) for AA contrast.
        assert_eq!(color::TEXT_DIM.l, 0.5490);
        assert_eq!(color::ICON.l, 0.5255);
        assert_eq!(color::DIVIDER.l, 0.1804);
        // The universal-selector border that every `border-*` actually paints.
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
