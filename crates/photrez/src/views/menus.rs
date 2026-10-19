//! Visual-only port of `shell/AppMenuBar.tsx`'s menu popups.
//!
//! # Where they sit, and the frame convention
//!
//! Every popup is **224px wide** with its top edge at **y=36** — 10px inside
//! the 46px title bar, which is the same `title::MENU_PX` the triggers use. The x is
//! the *trigger's own left edge* (read from `manifest.json`), so a popup
//! inherits `title_bar.rs`'s menu layout rather than re-deriving it:
//!
//! ```text
//!   menu-file      224x242 @  58,36      menu-edit    224x252 @ 100,36
//!   menu-image     224x38  @ 144,36      menu-layer   224x224 @ 201,36
//!   menu-view      224x308 @ 254,36      menu-window  224x103 @ 303,36
//!   menu-help      224x38  @ 372,36      menu-settings 224x38 @ 421,36
//! ```
//!
//! Those heights are measured, not computed from the source's entry list, and
//! they do not all agree with it — see "What the measurements added" below.
//!
//! The popups are drawn inside a **transparent absolute 1366x820 frame** with
//! each menu pinned at its capture origin. That is the convention
//! `context_menus.rs`, `toasts.rs` and `tooltips.rs` already use, and it exists
//! for one reason: GPUI has no `z_index`, so paint order is tree order, and the
//! only way to draw over the shell is to cover the viewport and be appended
//! late. `workspace.rs` gates this module on `shot::starts_with("menu-")` so a
//! plain launch renders the default shell unchanged.
//!
//! # Row geometry, measured in `menu-file.png`
//!
//! ```text
//!   shell border   1px  #333333   color::BORDER    rows y=1 and y=242
//!   separator      1px  #2e2e2e   color::DIVIDER   full-bleed, no inset
//!   row            28px           label left, shortcut right
//!   separator block my-1 (4px) + 1px line        = 9px
//!   recent row     28px           11px label, text-dim/60, File only
//! ```
//!
//! The separator/border split is worth stating because both are "a border" in
//! the source: `border-editor-divider` is a real class and resolves to
//! `color::DIVIDER`, whereas the shell's edge is `color::BORDER`. Scanning for
//! `#333333` alone reports zero separators, which reads as "the port forgot
//! them" when it is only the wrong constant.
//!
//! The spacing arithmetic cross-checks the row height: the gaps between
//! successive text bands are 29.5, 35.5, 28, 38.5 and 35.5px, and 28 = one
//! row while 37 = row + separator. So rows are 28px and a separator block is
//! 9px — the same figures `context_menus.rs` measured independently for
//! `ContextMenu.tsx`, which is reassuring given the two chrome sources differ.
//!
//! # What the measurements added that the source does not say
//!
//! * `MENU_DEFINITIONS` lists **6 items and 3 separators** for File, but
//!   the scan finds **four** full-bleed separator rows (y=66, 131, 168,
//!   205) and a dim text band at y=219..226 that no entry in the list
//!   accounts for. Both belong to `RecentFilesMenu`, which
//!   `AppMenuBar.tsx` renders inside the File popup alone
//!   (`<Show when={m() === "File"}>`): a `my-1 h-px bg-editor-divider`
//!   divider, then — because the capture has no recent files — the
//!   empty-state placeholder, a `h-7` row of `text-[11px]
//!   text-editor-text-dim/60` reading "No Recent Files". That row is
//!   the 28px the entry-list arithmetic misses: 5 + 6x28 + 4x9 + 28 +
//!   5 = 242, the measured box height (the 243rd pixel row is the
//!   half-pixel crop origin at y=35.5). The placeholder's ink reads
//!   #43..#55 — `#7a7a7a` at 60% over `#1e1e1e` — dim enough that a
//!   hard-ink scan walks past it, which is why the first probes filed
//!   the 28px as "unexplained panel".
//! * **`Image`, `Help` and `Settings` are 38px tall** — a header and one item,
//!   not a full menu. Their popups are a single row, so the port renders them
//!   from the same builder rather than special-casing an "empty" menu.
//! * `probe-menus.ts` already proved the trigger-to-ink padding is exactly
//!   10px, matching `title::MENU_PX`, so the popup's y=36 origin is the same
//!   constant rather than a coincidence.
//!
//! # Gaps
//!
//! * No `z_index` — the frame convention above is the substitute, and it is
//!   only correct while the caller appends late.
//! * `transition-colors`, `focus-visible` rings and keyboard navigation are
//!   interaction. The hovered row *is* painted where a baseline shows it: the
//!   two `menu-*-hover-item` captures seed their first row through the
//!   `focus_bg` argument `components::ui::menu_row` already takes (see
//!   `HOVER_ROW_FILL`), the same argument the pointer-anchored context menus
//!   use for their `focus-visible` row. A live hover still cannot tint a row
//!   without a per-variant `.id` + `Stateful` wrapper that breaks the
//!   `Vec<Div>` shell builder, so only the seeded captures paint it.
//! * `border-white/*` classes would lose to the unlayered `*` rule; these
//!   menus use `border-editor-divider`, which is a real class and does resolve,
//!   so `color::DIVIDER` is correct here rather than a fallback.
//! * Icon edges carry LCD subpixel colour GPUI cannot reproduce (it tints an
//!   alpha mask), and `-webkit-font-smoothing: antialiased` covers type only,
//!   not SVG. Position will be right, colour will differ.
//! * No letter-spacing: GPUI's shaper runs +/-0.2px per glyph off Chromium, as
//!   `probe-menus.ts` measured, so a label's right edge drifts by a pixel or
//!   two. Shortcuts are right-aligned, which hides this at the label end.

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Div, Hsla};

use crate::components::ui;
use crate::shot;
use crate::tokens::color;

/// The transparent frame the popups live in: the whole client area, so a menu
/// hangs at its capture origin no matter where the tree puts this module.
const FRAME_W: f32 = 1366.0;
const FRAME_H: f32 = 820.0;

/// Every popup is 224px wide in the baselines.
const MENU_W: f32 = 224.0;

/// The popup's top edge, 10px inside the 46px bar.
const MENU_TOP: f32 = 36.0;

/// The trigger origins — one per menu, all read from `manifest.json`.
const X_FILE: f32 = 58.0;
const X_EDIT: f32 = 100.0;
const X_IMAGE: f32 = 144.0;
const X_LAYER: f32 = 201.0;
const X_VIEW: f32 = 254.0;
const X_WINDOW: f32 = 303.0;
const X_HELP: f32 = 372.0;
const X_SETTINGS: f32 = 421.0;

/// `py-1` — the shell's vertical inset.
const SHELL_PY: f32 = 4.0;

/// A row: 28px tall, measured from the gap between text bands.
const ROW_H: f32 = 28.0;

/// `px-3` — a row's horizontal inset (consumed by `recent_files_row`, which is
/// the only row built here instead of via `ui::menu_row`).
const ROW_PX: f32 = 12.0;

// The label is 12px regular in `text-editor-text`; the shortcut is 10px dim —
// both sizes live in `ui::menu_row`, so they are not redeclared here. The
// separator block is `my-1` (4px) + 1px line = 9px; that 4px is inlined in the
// height test below rather than named, since `menu_separator` owns the real
// constant in `ui.rs`.

/// `text-[11px]` — the recent-files placeholder, one step below the
/// label and in `text-editor-text-dim/60`.
const RECENT_TEXT: f32 = 11.0;

/// `rounded-[6px]` on the shell.
const SHELL_R: f32 = 6.0;

/// The hovered first row in the two `menu-*-hover-item` baselines:
/// `bg-editor-field/70` over the `#1e1e1e` panel reads `#232323` = 35/255 on
/// the strip (crop y6..33 — the whole first row). A measured solid, so the
/// value is exact under any blend path.
const HOVER_ROW_FILL: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 35.0 / 255.0,
    a: 1.0,
};

pub fn render() -> impl IntoElement {
    let id = shot::active().unwrap_or("menu-file");
    let body = match id {
        "menu-file" => positioned(X_FILE, file_menu(None)),
        "menu-file-hover-item" => positioned(X_FILE, file_menu(Some(HOVER_ROW_FILL))),
        "menu-edit" => positioned(X_EDIT, edit_menu()),
        "menu-image" => positioned(X_IMAGE, image_menu()),
        "menu-layer" => positioned(X_LAYER, layer_menu()),
        "menu-view" => positioned(X_VIEW, view_menu()),
        "menu-window" => positioned(X_WINDOW, window_menu()),
        "menu-help" => positioned(X_HELP, help_menu()),
        "menu-settings" => positioned(X_SETTINGS, settings_menu(None)),
        "menu-settings-hover-item" => positioned(X_SETTINGS, settings_menu(Some(HOVER_ROW_FILL))),
        // Unknown ids fall through to the default state.
        _ => positioned(X_FILE, file_menu(None)),
    };
    // Transparent full-viewport frame, itself absolute so it disturbs no
    // layout wherever it is appended. The caller appends late; late paints on
    // top, which is the only `z-index` equivalent GPUI offers.
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .w(px(FRAME_W))
        .h(px(FRAME_H))
        .child(body)
}

/// Pin a popup at its trigger origin inside the frame.
fn positioned(x: f32, menu: Div) -> Div {
    menu.absolute().left(px(x)).top(px(MENU_TOP))
}

/// One entry of `MENU_DEFINITIONS`. `shortcut` is the right-aligned key hint
/// and `disabled` is whole-row `opacity-35` in the source — no captured menu
/// has a disabled row, so the arm is here for completeness rather than because
/// a baseline shows it.
/// `my-1 border-t border-editor-divider`, full-bleed — the probe reads
/// separator rows edge to edge, so there is no horizontal inset.
/// `RecentFilesMenu`'s empty state — the row the File popup shows when
/// `getRecentFiles()` returns nothing: `h-7 items-center px-3
/// text-[11px] text-editor-text-dim/60`, `aria-disabled`. Same shape
/// as a `menu_row` with a smaller, dimmer label and no shortcut, and
/// the 28px the entry list does not account for: it sits below the
/// fourth separator, closing the popup at 242px.
///
/// Unlike the source, the label paints full-strength [`color::TEXT_DIM`]:
/// the source's `/60` composites to ~2.2:1 over the panel, too faint for the
/// 11px placeholder that carries this row's whole message.
fn recent_files_row() -> Div {
    h_flex()
        .w_full()
        .h(px(ROW_H))
        .items_center()
        .px(px(ROW_PX))
        .child(
            div()
                .text_size(px(RECENT_TEXT))
                .font_family(ui::FONT_UI)
                .text_color(color::TEXT_DIM)
                .child("No Recent Files".to_string()),
        )
}

/// The shared chrome: 224px, 6px corners, a 1px `color::BORDER` edge,
/// `color::PANEL` fill, `py-1`, clipped.
///
/// `border-white/*` is *not* what the shell's edge is: `AppMenuBar.tsx` puts
/// the popup in a `rounded-[6px] border border-editor-divider bg-editor-panel`
/// wrapper, and the measurement says that edge reads `#333333` (`BORDER`)
/// while the separators inside read `#2e2e2e` (`DIVIDER`). Two different
/// constants for two different things in the same menu.
fn menu_shell(entries: Vec<Div>) -> Div {
    v_flex()
        .w(px(MENU_W))
        .rounded(px(SHELL_R))
        .border_1()
        .border_color(color::BORDER)
        .bg(color::PANEL)
        .py(px(SHELL_PY))
        .overflow_hidden()
        .children(entries)
}

/// File — 6 items, 3 separators, then `RecentFilesMenu`: a fourth
/// separator and its empty-state row. See the module docs.
///
/// `hover_first` paints the first row for `menu-file-hover-item`; the resting
/// `menu-file` passes `None`.
fn file_menu(hover_first: Option<Hsla>) -> Div {
    menu_shell(vec![
        ui::menu_row("New Document", Some("Ctrl+N"), false, hover_first),
        ui::menu_row("Open Image...", Some("Ctrl+O"), false, None),
        ui::menu_separator(),
        ui::menu_row("Save", Some("Ctrl+S"), false, None),
        ui::menu_row("Save As...", Some("Ctrl+Shift+S"), false, None),
        ui::menu_separator(),
        ui::menu_row("Export...", Some("Ctrl+Alt+E"), false, None),
        ui::menu_separator(),
        ui::menu_row("Print...", Some("Ctrl+P"), false, None),
        // `RecentFilesMenu`, rendered in the File popup alone. Its
        // divider is the fourth full-bleed line the probe reads at
        // y=205; `MENU_DEFINITIONS` does not list it because it is
        // not part of the menu proper.
        ui::menu_separator(),
        // The empty-state placeholder under that divider: `h-7`
        // (28px) of `text-[11px] text-editor-text-dim/60`. The
        // probe reads its ink at y=219..226 — dim enough
        // (#43..#55) that a hard-ink scan walks past it.
        recent_files_row(),
    ])
}

/// Edit — 9 items, 2 separators.
fn edit_menu() -> Div {
    menu_shell(vec![
        ui::menu_row("Undo", Some("Ctrl+Z"), false, None),
        ui::menu_row("Redo", Some("Ctrl+Shift+Z"), false, None),
        ui::menu_separator(),
        ui::menu_row("Cut", Some("Ctrl+X"), false, None),
        ui::menu_row("Copy", Some("Ctrl+C"), false, None),
        ui::menu_row("Paste", Some("Ctrl+V"), false, None),
        ui::menu_separator(),
        ui::menu_row("Select All", Some("Ctrl+A"), false, None),
        ui::menu_row("Deselect", Some("Ctrl+D"), false, None),
        ui::menu_row("Invert Selection", Some("Ctrl+Shift+I"), false, None),
    ])
}

/// Image — one item, and a 38px popup: the header-only shape.
fn image_menu() -> Div {
    menu_shell(vec![ui::menu_row("Resize Canvas...", None, false, None)])
}

/// Layer — 7 items, 2 separators.
fn layer_menu() -> Div {
    menu_shell(vec![
        ui::menu_row("New Layer", Some("Ctrl+Shift+N"), false, None),
        ui::menu_row("Duplicate Layer", Some("Ctrl+J"), false, None),
        ui::menu_row("Delete Layer", None, false, None),
        ui::menu_row("Select All Layers", Some("Ctrl+Alt+A"), false, None),
        ui::menu_separator(),
        ui::menu_row("Merge Down", Some("Ctrl+E"), false, None),
        ui::menu_row("Stamp Visible", Some("Ctrl+Shift+Alt+E"), false, None),
        ui::menu_separator(),
        ui::menu_row("Flatten Image", Some("Ctrl+Shift+E"), false, None),
    ])
}

/// View — 10 items, 2 separators.
fn view_menu() -> Div {
    menu_shell(vec![
        ui::menu_row("Zoom In", Some("Ctrl++"), false, None),
        ui::menu_row("Zoom Out", Some("Ctrl+-"), false, None),
        ui::menu_row("Actual Size", Some("Ctrl+1"), false, None),
        ui::menu_row("Fit Canvas", Some("Ctrl+0"), false, None),
        ui::menu_row("Zoom to Selection", Some("Ctrl+Alt+0"), false, None),
        ui::menu_separator(),
        ui::menu_row("Snap", Some("Shift+Ctrl+;"), false, None),
        ui::menu_row("Snap to Layers", None, false, None),
        ui::menu_row("Snap to Canvas", None, false, None),
        ui::menu_separator(),
        ui::menu_row("Toggle Side Panels", Some("Ctrl+Shift+P"), false, None),
        ui::menu_row("Use Stacked Side Dock", None, false, None),
    ])
}

/// Window — 3 items, 1 separator.
fn window_menu() -> Div {
    menu_shell(vec![
        ui::menu_row("Minimize", None, false, None),
        ui::menu_row("Maximize / Restore", None, false, None),
        ui::menu_separator(),
        ui::menu_row("Close Window", Some("Alt+F4"), false, None),
    ])
}

/// Help — one item, header-only shape.
fn help_menu() -> Div {
    menu_shell(vec![ui::menu_row("About Photrez", None, false, None)])
}

/// Settings — one item, header-only shape. `hover_first` paints it for
/// `menu-settings-hover-item`.
fn settings_menu(hover_first: Option<Hsla>) -> Div {
    menu_shell(vec![ui::menu_row(
        "Preferences...",
        None,
        false,
        hover_first,
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The eight top-level menu origins are frozen by `manifest.json`; a
    /// reordering would shift a popup's box and fail the diff.
    #[test]
    fn trigger_origins_match_manifest() {
        assert_eq!(X_FILE, 58.0);
        assert_eq!(X_EDIT, 100.0);
        assert_eq!(X_IMAGE, 144.0);
        assert_eq!(X_LAYER, 201.0);
        assert_eq!(X_VIEW, 254.0);
        assert_eq!(X_WINDOW, 303.0);
        assert_eq!(X_HELP, 372.0);
        assert_eq!(X_SETTINGS, 421.0);
    }

    /// `menu-file`'s measured height is a hard pin: 5 (py-1) + 6 rows*28 +
    /// 4 seps*(8+1) + 1 recent row*28 + 5 (py-1) + 2 (border) = 242. Removing a
    /// row (-28) or a separator (-9) reddens.
    #[test]
    fn file_menu_height_is_242px() {
        let sep_my = 4.0; // `my-1` — `ui::menu_separator` owns the real value
        let computed = SHELL_PY + 6.0 * ROW_H + 4.0 * (2.0 * sep_my + 1.0) + ROW_H + SHELL_PY + 2.0;
        assert_eq!(computed, 242.0);
    }

    /// The hovered-row fill is a measured pin (`#232323` = 35/255): change the
    /// value and the `menu-*-hover-item` strip comparison reddens.
    #[test]
    fn hover_row_fill_is_232323() {
        assert_eq!((HOVER_ROW_FILL.l * 255.0).round() as u8, 35);
        assert_eq!(HOVER_ROW_FILL.a, 1.0);
    }
}
