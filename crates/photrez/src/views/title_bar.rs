//! Visual-only port of `AppTitleBar.tsx`, including the closed-state
//! `AppMenuBar.tsx` it hosts.
//!
//! This is the second component the spike ports, and deliberately a different
//! shape from [`tool_rail`]: a 46px horizontal bar of **text** rather than a
//! vertical column of boxes. Where the rail exercised layout primitives, this
//! exercises typography, fractional padding and an SVG that is not monochrome.
//!
//! # Scope: the resting state only
//!
//! Captured baseline: `shell-titlebar.png` (1366x46) and `shell-menubar.png`
//! (the 8 triggers). Not ported, because none of it can appear in a static
//! screenshot of the resting state:
//!
//! * the menu popups themselves (Portal, `z-[100]`, keyboard navigation)
//! * the system menu on Alt+Space
//! * `data-tauri-drag-region` and its double-click-to-maximize
//! * window state (`isMaximized()` swaps the maximize icon)
//!
//! Hover states **are** ported, for the same reason the rail's are: they cost
//! nothing and they are part of the component's contract.
//!
//! # Three fidelity gaps
//!
//! ## 1. Letter-spacing does not exist in GPUI (measured, still open)
//!
//! `AppMenuBar` renders each trigger at `text-[12px] ... tracking-wide`, i.e.
//! `letter-spacing: 0.025em` = 0.3px **after every glyph, including the last**.
//! GPUI's text system has no letter-spacing anywhere in its pipeline
//! (`LineLayout`/`ShapedGlyph` carry no such field; `Styled` exposes no
//! `tracking`).
//!
//! Triggers are laid out inline, so each one starts where the previous one
//! ended and the deficit **accumulates** across the bar rather than staying
//! local to a label. That makes it measurable: take the original's trigger
//! origins from the `menu-*` boxes in `baseline/shots/manifest.json` (a
//! dropdown aligns to its own trigger, so its `x` is the trigger's `x`), take
//! the port's from `probe-menus.ts`, and subtract.
//!
//! ```text
//! trigger    original    port    delta   glyphs
//! File         58.00      58    0.00       4
//! Edit         99.88      99   -0.88       4
//! Image       144.00     142   -2.00       5
//! Layer       201.47     198   -3.47       5
//! View        253.94     249   -4.94       4
//! Window      303.44     298   -5.44       6
//! Help        372.44     367   -5.44       4
//! Settings    421.20     415   -6.20       -
//! ```
//!
//! Two things fall out of that table.
//!
//! **The padding is right.** In both images the ink begins exactly 10.0px
//! inside its trigger — `px-2.5`, [`title::MENU_PX`] — so [`pad_x`] needs no
//! correction, and File starting at 58 with a delta of 0 rules out a constant
//! offset anywhere before the bar.
//!
//! **The deficit is 0.194px per glyph**, summed over the 32 glyphs before
//! Settings. Of that, `tracking-wide` accounts for the bulk: dropping 0.3px
//! per glyph would be −9.6px. The remaining **+0.106px per glyph runs the
//! other way** — GPUI's shaper resolves slightly wider advances than
//! Chromium's — and it is character-dependent rather than uniform (Window and
//! Help each come out level, Image and Layer do not), which is the signature
//! of per-glyph advance rounding rather than a scaling factor.
//!
//! The two effects are why this reads as −0.19 rather than −0.3: **the port is
//! not simply the original minus letter-spacing.** Restoring letter-spacing
//! exactly would overshoot to roughly +3px of accumulated drift, because the
//! shaper difference is already pushing the other way.
//!
//! A compensation was tried and reverted on the strength of this asymmetry,
//! measured as `+9px / HARD 0.127%` against the uncompensated `-6px / HARD
//! 0.099%`. The attempt over-corrected by more than the tracking it added: it
//! split the correction across both sides of the padding, and GPUI lays a
//! fractional `px()` down as a whole pixel (`10.6` painted as `11`), so each
//! trigger gained ~0.8px it had not asked for before the tracking was even
//! counted.
//!
//! [`pad_x`] is therefore plain `px-2.5`, and still takes the label though
//! nothing reads it: the labels are localised (`t(MENU_LABEL_KEYS[menu])`), so
//! any retry has to be computed for the string actually being drawn, and has
//! to put the correction somewhere other than fractional padding.
//!
//! The −6px is cosmetic and local to this bar — `HARD 0.099%` on the region,
//! one pixel's worth of hard mismatch — so it is recorded rather than chased.
//! It is worth knowing for the rest of the port that the per-glyph term is
//! real but small: it only compounds where content-width elements sit in a
//! row (a menu bar, a tab strip), and there it is sub-pixel per item.
//!
//! The document title and the "photrez" wordmark also carry tracking, but both
//! are *centred* inside a `flex-1` cell, so their boxes do not position anything
//! and only a 0.15px sub-pixel offset is at stake. They are left alone.
//!
//! ## 2. The logo cannot go through `Icon` (worked around)
//!
//! `Icon` ends in `paint_svg`, which GPUI documents as "Paint a **monochrome**
//! SVG into the scene": it keeps the alpha channel and tints it with
//! `text_color`. The Photrez mark is two linear gradients plus a mask plus an
//! `feDropShadow`, so the tint would flatten it to one colour. It is rendered
//! with `img(ImageSource::Render(..))` instead, which routes through
//! `render_single_frame` and keeps full colour.
//!
//! The raster is produced once in [`init`] rather than per frame.
//!
//! ## 3. Icon hover tint (not compensated)
//!
//! Same gap as `tool_rail`: `Icon` resolves its tint while the element tree is
//! built, before hover styles apply, so undo/redo/window icons do not brighten
//! on hover — only the button background does. No baseline captures a title-bar
//! hover, so this is unverified either way.
//!
//! # The window controls are per-platform, by design
//!
//! The title bar is the only title bar — the native one is hidden — so the
//! minimise/maximise/close affordance has to be drawn here, and each OS expects
//! its own convention rather than Windows': AppKit's traffic lights on macOS, a
//! server-side decoration that should simply *not be drawn* on a Linux
//! compositor that owns the frame, and the Windows `WindowControlArea` hit
//! regions elsewhere. The three-way branch lives in [`window_controls`], which
//! documents what each arm does and why; [`title_bar::render`] threads
//! `&mut Window` down to it purely so that branch can ask the window how it is
//! decorated.

use std::sync::{Arc, OnceLock};

use gpui_kit::base::h_flex;
use gpui_kit::component::Icon;
// `StyledExt` is what supplies `.h_flex()` as a *method*. The free function
// `base::h_flex()` only builds a bare `Div`, but `div().id(..)` returns
// `Stateful<Div>` — the id is what makes the button hit-testable, and therefore
// what makes `.hover()` fire — and that type needs the trait in scope.
// (`gpui_kit::base` is `pub use ::gpui_base as base`, so the trait lives there
// alongside `h_flex`/`v_flex`.)
use gpui_kit::base::StyledExt;
use gpui_kit::prelude::*;
use gpui_kit::{
    div, img, px, Decorations, FontWeight, ImageSource, RenderImage, SvgRenderer, Window,
    WindowControlArea,
};

use crate::assets::icons;
use crate::components::ui;
use crate::i18n::t;
use crate::tokens::{color, title};

/// The eight top-level menus, in `MENU_ITEMS` order from `editorData.ts`,
/// as catalog keys resolved with [`t`] at the trigger.
/// Order matters: the measured trigger boxes in `manifest.json` are File at
/// x=58, Edit at x=99.9, and so on, so a reordering would fail the diff.
const MENUS: &[&str] = &[
    "menus.file",
    "menus.edit",
    "menus.image",
    "menus.layer",
    "menus.view",
    "menus.window",
    "menus.help",
    "menus.settings",
];

/// `activeDocName()` for the captured state: `capture-baseline.ts:183` creates
/// the document via `WorkspaceManager.createBlankDocument("capture-baseline",
/// "Baseline", 800, 600)`, so `displayName` is "Baseline".
const DOC_NAME: &str = "Baseline"; // i18n-exempt: demo content

/// The logo raster. Filled by [`init`] before any window opens, so [`logo`]
/// can rely on it being present.
static LOGO_IMAGE: OnceLock<Arc<RenderImage>> = OnceLock::new();

/// Rasterises the brand mark exactly once, before the first frame.
///
/// `render_single_frame(.., 1.0)` maps to `SvgSize::ScaleFactor`, which this
/// GPUI multiplies by `SMOOTH_SVG_SCALE_FACTOR` (2) — so the logo's 30x30
/// intrinsic size is rasterised at 60x60 and drawn at 30x30, the same 2x
/// supersampling `paint_svg` applies to the lucide icons.
///
/// Panics with the underlying `usvg` error rather than rendering nothing: a
/// silently missing logo would show up as a confusing diff, and if usvg cannot
/// handle this SVG the port needs a different strategy for it.
#[expect(
    clippy::panic,
    reason = "logo raster failure is startup-fatal; a silently missing logo is a defect"
)]
pub fn init(svg: &SvgRenderer) {
    let image = svg
        .render_single_frame(icons::LOGO, 1.0)
        .unwrap_or_else(|e| {
            panic!(
                "the Photrez logo SVG failed to rasterise: {e}\n\
                 gradient/mask/filter support is the likely culprit"
            )
        });
    let _ = LOGO_IMAGE.set(image);
}

/// `<header class="relative flex h-[46px] shrink-0 items-center border-b
/// border-editor-divider bg-editor-topbar select-none overflow-hidden">`
///
/// `window` is threaded down only as far as `right_group`, which needs it to
/// ask whether this window is actually client-side decorated — a Linux-only
/// question, but answered through the same call on every platform so the
/// signature does not fork.
pub fn render(window: &mut Window) -> impl IntoElement {
    h_flex()
        .relative()
        .h(px(title::HEADER_H))
        .flex_shrink_0()
        // `h_flex()` already sets items_center; the explicit call documents
        // that the three groups below are centred on the cross axis unless
        // they opt into `.self_stretch()`.
        .items_center()
        .border_b_1()
        // `border-editor-divider` names #2e2e2e, but the unlayered `*` in
        // styles.css outranks it and paints #333333 — see `color::BORDER`,
        // which records the four baselines proving it.
        .border_color(color::BORDER)
        .bg(color::TOPBAR)
        .overflow_hidden()
        // The whole bar is Inter. GPUI's theme default is `.SystemUIFont`,
        // which resolves to Segoe UI here — a different font with different
        // advances, which would move every trigger in the menu bar.
        .font_family(ui::FONT_UI)
        .child(left_group())
        .child(center_group())
        .child(right_group(window))
}

/// `<div class="flex min-w-0 items-center gap-2 self-stretch">` — logo cell
/// plus the menu bar.
fn left_group() -> impl IntoElement {
    h_flex()
        .min_w(px(0.))
        .items_center()
        .gap(px(title::LEFT_GAP))
        .self_stretch()
        .child(logo_cell())
        .child(menu_bar())
}

/// `<div class="flex h-full shrink-0 items-center pl-3 pr-2">`
///
/// The `h-full` is why the wrapper is stretched rather than centred: it is
/// what gives the logo cell the bar's full 45px content height to centre in.
fn logo_cell() -> impl IntoElement {
    // On macOS the traffic lights own the top-left corner, so the logo starts
    // after them instead of at `pl-3`. See `title::LOGO_PAD_L_MACOS` for why
    // and for the fact that the clearance is an estimate.
    let pad_left = if cfg!(target_os = "macos") {
        title::LOGO_PAD_L_MACOS
    } else {
        title::LOGO_PAD_L
    };

    h_flex()
        .self_stretch()
        .flex_shrink_0()
        .items_center()
        .pl(px(pad_left))
        .pr(px(title::LOGO_PAD_R))
        .child(logo(title::LOGO))
}

/// `<svg viewBox="0 0 512 512" class="size-[30px] shrink-0">` — full colour,
/// see the module docs for why this is `img` and not `Icon`.
///
/// `size` is the box the call site asks for: [`title::LOGO`] in the bar, a
/// larger mark in the About dialog. Both draw the same 60px raster, so the
/// brand image stays one image with one copy of the gradient/mask/filter
/// artwork.
#[expect(clippy::expect_used)] // init() is the parent's wiring responsibility; a missing raster is a programmer error, not a runtime one
pub(crate) fn logo(size: f32) -> impl IntoElement {
    let image = LOGO_IMAGE
        .get()
        .expect("title_bar::init() must run before the window opens");

    img(ImageSource::Render(image.clone()))
        .size(px(size))
        .flex_shrink_0()
}

/// `<nav class="hidden h-full items-center gap-0.5 md:flex">`
///
/// `hidden ... md:flex` is simply `flex` at this viewport: the baseline is
/// 1366px wide, comfortably past the 768px `md` breakpoint.
fn menu_bar() -> impl IntoElement {
    h_flex()
        .self_stretch()
        .items_center()
        .gap(px(title::MENU_GAP))
        .children(MENUS.iter().copied().map(menu_trigger))
}

/// One menu trigger: `<div class="relative flex h-full items-center">` wrapping
/// the button itself.
///
/// `key` is the catalog key: it names the element id and resolves the visible
/// label, so the id stays locale-independent while the text follows the locale.
///
/// The button is not given a popup — see the module docs for why the open
/// state is out of scope.
fn menu_trigger(key: &'static str) -> impl IntoElement {
    h_flex().relative().self_stretch().items_center().child(
        div()
            .id(key)
            .h_flex()
            .h(px(title::MENU_H))
            .items_center()
            .justify_center()
            .rounded(px(title::RADIUS))
            .px(px(pad_x(key)))
            .text_size(px(title::MENU_TEXT))
            // `font-medium`
            .font_weight(FontWeight(500.))
            // `text-editor-text/90`
            .text_color(color::TEXT_90)
            // `hover:bg-white/[0.045] hover:text-editor-text`
            .hover(|s| s.bg(color::WHITE_45).text_color(color::TEXT))
            .child(t(key)),
    )
}

/// Horizontal padding for a menu trigger.
///
/// Plain `px-2.5` — and the tracking compensation this used to carry is gone.
/// See the module docs; it was based on a wrong assumption about where
/// `letter-spacing` lands, and removing it is what the measurement says.
fn pad_x(_label: &str) -> f32 {
    title::MENU_PX
}

/// `<div class="flex min-w-4 flex-1 items-center justify-center self-stretch">`
///
/// The `data-tauri-drag-region` attribute and its double-click handler have no
/// native equivalent here; the cell is still a plain centred flex child.
fn center_group() -> impl IntoElement {
    h_flex()
        .min_w(px(title::CENTER_MIN_W))
        .flex_1()
        .items_center()
        .justify_center()
        .self_stretch()
        .child(
            h_flex()
                .items_center()
                .gap(px(title::CENTER_GAP))
                // `opacity-90` — applied to the row, not to each span.
                .opacity(0.9)
                .child(
                    div()
                        .text_size(px(title::MENU_TEXT))
                        // `font-semibold`
                        .font_weight(FontWeight(600.))
                        .text_color(color::TEXT)
                        // `tracking-tight` left uncompensated: this row is
                        // centred, so the missing tracking costs 0.15px.
                        .child(DOC_NAME),
                )
                .child(
                    div()
                        .text_size(px(title::SEP_TEXT))
                        .text_color(color::TEXT_DIM_60)
                        .child("\u{b7}"), // i18n-exempt: glyph or symbol
                )
                .child(
                    div()
                        .text_size(px(title::WORDMARK_TEXT))
                        .font_weight(FontWeight(500.))
                        // Source asks for `text-editor-text-dim/80`; 80%
                        // composites to ~3.2:1 over the topbar, so the
                        // wordmark paints at the full dim strength. The
                        // separator dot stays at 60%: punctuation, exempt
                        // as decoration.
                        .text_color(color::TEXT_DIM)
                        .child("photrez"), // i18n-exempt: product name
                ),
        )
}

/// `<div class="flex min-w-0 items-center">` — undo/redo plus window controls.
fn right_group(window: &mut Window) -> impl IntoElement {
    h_flex()
        .min_w(px(0.))
        .items_center()
        .child(undo_group())
        .child(window_controls(window))
}

/// `<div class="mr-3 flex shrink items-center gap-0.5 overflow-hidden
/// text-editor-icon">`
///
/// The `panel-right-close` / `panel-right-open` toggle that follows the rule is
/// `lg:hidden`, so at the 1366px baseline it is `display:none` and occupies no
/// space — which is why it is absent rather than merely omitted. That checks
/// out arithmetically: this group is 81px wide (28 + 2 + 28 + 2 + 4 + 1 + 4 +
/// 12), and 1366 - 81 - 3*44 = 1153, exactly where `manifest.json` measured
/// the undo button.
fn undo_group() -> impl IntoElement {
    h_flex()
        .mr(px(title::UNDO_MR))
        .items_center()
        .gap(px(title::UNDO_GAP))
        .overflow_hidden()
        .text_color(color::ICON)
        .child(icon_button("undo", icons::UNDO))
        .child(icon_button("redo", icons::REDO))
        .child(rule())
}

/// `flex size-7 items-center justify-center rounded-[4px]
///  hover:bg-white/[0.045] hover:text-editor-text`
fn icon_button(id: &'static str, icon: &'static [u8]) -> impl IntoElement {
    div()
        .id(id)
        .h_flex()
        .size(px(title::UNDO_BTN))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(title::RADIUS))
        .hover(|s| s.bg(color::WHITE_45).text_color(color::TEXT))
        .child(
            Icon::default()
                .data(icon)
                .size(px(title::ICON_16))
                // The tint is decided here rather than inherited, because
                // `Icon` reads `window.text_style()` while the tree is built —
                // before hover refinement. Same constraint as `tool_rail`.
                .text_color(color::ICON),
        )
}

/// `<div class="mx-1 h-3.5 w-px bg-editor-divider" />`
fn rule() -> impl IntoElement {
    div()
        .mx(px(title::RULE_MX))
        .h(px(title::RULE_H))
        .w(px(1.))
        .bg(color::DIVIDER)
}

/// `<div class="flex shrink-0 items-center pr-0 text-editor-icon">` and its
/// three `h-[46px] w-11` children — **on the platforms that have such children**.
///
/// Each button is the full 46px header height while the content box is 45px,
/// so they overhang by 0.5px top and bottom and are clipped by the header's
/// `overflow-hidden`. `manifest.json` recorded exactly that: Minimize at
/// `y = -0.5`, height 46.
///
/// # The three platforms do not share a mechanism
///
/// Photrez's original paints the same three right-hand buttons everywhere,
/// because Tauri's `decorations: false` strips window chrome uniformly and
/// leaves the web layer to invent it. Native toolkits do not work that way,
/// so this follows each platform's convention instead of cloning the Windows
/// layout onto all of them:
///
/// | platform | what owns the controls |
/// |---|---|
/// | Windows | these three, but driven by the OS via hit-test codes |
/// | macOS | AppKit's traffic lights — this renders an empty cell |
/// | Linux, client-decorated | these three, driven by imperative `Window` calls |
/// | Linux, server-decorated | the window manager — empty cell again |
///
/// The Linux split is load bearing: a compositor may deny client-side
/// decorations, or fall back to server-side after granting them, and the WM
/// then draws its own minimise/maximise/close. Painting ours as well stacks
/// two sets — most visibly two close buttons — so the cell is emptied unless
/// `window_decorations()` actually reports `Client`.
///
/// # Why Windows alone uses `window_control_area`
///
/// It is not a click handler. It records which region sits under the cursor,
/// and `WM_NCHITTEST` then answers `HTMINBUTTON` / `HTMAXBUTTON` /
/// `HTCLOSE`, so *Windows* performs the action and supplies hover, the
/// snap-layouts fly-out, and accessibility semantics — none of which a GPUI
/// `on_click` reproduces. It is also the only platform where that works:
/// `on_hit_test_window_control` is an empty body in `gpui-pre-macos`,
/// `gpui-pre-linux/x11` and `gpui-pre-linux/wayland`, so those two branch to
/// explicit calls instead.
///
/// On Windows these buttons are pixel-identical to before this branching
/// existed — `window_control_area` registers hit-testing, it does not paint.
fn window_controls(window: &mut Window) -> impl IntoElement {
    let cell = h_flex()
        .id("window-controls")
        .flex_shrink_0()
        .items_center()
        .text_color(color::ICON);

    // macOS: the traffic lights are the window controls. They cannot be
    // suppressed — GPUI only ever sets `titlebarAppearsTransparent` and hides
    // the title, leaving `NSTitledWindowMask` in place — so drawing our own
    // would put six buttons on screen.
    if cfg!(target_os = "macos") {
        return cell;
    }

    // Linux under server-side decorations: the WM already drew them.
    // `cfg!` short-circuits, so `window_decorations()` is never reached on
    // Windows, where the platform does not override it and would report
    // `Server` even though Windows draws nothing of the sort.
    if cfg!(target_os = "linux") && matches!(window.window_decorations(), Decorations::Server) {
        return cell;
    }

    cell.child(window_button(
        "minimize-window",
        icons::MINUS,
        title::ICON_15,
        WindowControlArea::Min,
    ))
    .child(window_button(
        "maximize-window",
        icons::SQUARE,
        title::ICON_12,
        WindowControlArea::Max,
    ))
    .child(close_button())
}

/// Issue a window-management action imperatively.
///
/// Only the Linux branch calls this. Windows answers it inside
/// `WM_NCHITTEST` and macOS inside AppKit, so neither needs a handler.
fn run_window_action(area: WindowControlArea, window: &mut Window) {
    match area {
        WindowControlArea::Min => window.minimize_window(),
        WindowControlArea::Max => window.zoom_window(),
        WindowControlArea::Close => window.remove_window(),
        WindowControlArea::Drag => window.start_window_move(),
    }
}

/// `flex h-[46px] w-11 items-center justify-center
///  hover:bg-white/[0.055] hover:text-editor-text`
fn window_button(
    id: &'static str,
    icon: &'static [u8],
    icon_size: f32,
    area: WindowControlArea,
) -> impl IntoElement {
    div()
        .id(id)
        .h_flex()
        .h(px(title::WINDOW_BTN_H))
        .w(px(title::WINDOW_BTN_W))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .hover(|s| s.bg(color::WHITE_55).text_color(color::TEXT))
        // Windows: hand the hit test over — see `window_controls`.
        .when(cfg!(target_os = "windows"), move |this| {
            this.window_control_area(area)
        })
        // Linux: the hit-test callback is a stub there, so act on the click.
        .when(cfg!(target_os = "linux"), move |this| {
            this.on_click(move |_, window, _| run_window_action(area, window))
        })
        .child(
            Icon::default()
                .data(icon)
                .size(px(icon_size))
                .text_color(color::ICON),
        )
}

/// `flex h-[46px] w-11 items-center justify-center hover:bg-red-500/85
///  hover:text-white`
fn close_button() -> impl IntoElement {
    div()
        .id("close-window")
        .h_flex()
        .h(px(title::WINDOW_BTN_H))
        .w(px(title::WINDOW_BTN_W))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .hover(|s| s.bg(color::RED_500_85).text_color(color::TEXT))
        .when(cfg!(target_os = "windows"), |this| {
            this.window_control_area(WindowControlArea::Close)
        })
        .when(cfg!(target_os = "linux"), |this| {
            this.on_click(|_, window, _| run_window_action(WindowControlArea::Close, window))
        })
        .child(
            Icon::default()
                .data(icons::X)
                .size(px(title::ICON_16))
                .text_color(color::ICON),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `pad_x` is the per-trigger horizontal padding for menu triggers.
    ///
    /// The reference measured all triggers starting their ink at exactly
    /// 10.0px inside the trigger box (`px-2.5`), which is `title::MENU_PX`.
    /// RED proof: change `pad_x` to return anything other than `MENU_PX` and
    /// this fails — the label argument is intentionally ignored because the
    /// baseline has no per-label padding variation.
    #[test]
    fn pad_x_returns_menu_px_for_any_label() {
        assert_eq!(pad_x("File"), title::MENU_PX);
        assert_eq!(pad_x("Settings"), title::MENU_PX);
        assert_eq!(pad_x(""), title::MENU_PX);
    }

    /// The eight menu labels are frozen by the captured baseline: `File` at
    /// x=58, `Settings` at x=421.20. A reordering fails the diff. The array
    /// holds catalog keys; the ends resolve to the captured labels.
    #[test]
    fn menus_array_is_frozen_order() {
        assert_eq!(
            MENUS,
            &[
                "menus.file",
                "menus.edit",
                "menus.image",
                "menus.layer",
                "menus.view",
                "menus.window",
                "menus.help",
                "menus.settings",
            ]
        );
        assert_eq!(t(MENUS[0]), "File");
        assert_eq!(t(MENUS[7]), "Settings");
    }
}
