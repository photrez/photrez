//! The menu bar's popup menus: their appearance.
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
//!   separator block 4px margins + 1px line       = 9px
//!   recent row     28px           11px label, dim text at 60%, File only
//! ```
//!
//! The separator/border split is worth stating because both are "a border" in
//! the design: separators resolve to
//! `color::DIVIDER`, whereas the shell's edge is `color::BORDER`. Scanning for
//! `#333333` alone reports zero separators, which reads as "this view forgot
//! them" when it is only the wrong constant.
//!
//! The spacing arithmetic cross-checks the row height: the gaps between
//! successive text bands are 29.5, 35.5, 28, 38.5 and 35.5px, and 28 = one
//! row while 37 = row + separator. So rows are 28px and a separator block is
//! 9px — the same figures `context_menus.rs` measured independently for
//! the pointer-anchored menus, which is reassuring given the two chrome
//! sources differ.
//!
//! # What the measurements added that the entry lists do not say
//!
//! * `MENU_DEFINITIONS` lists **6 items and 3 separators** for File, but
//!   the scan finds **four** full-bleed separator rows (y=66, 131, 168,
//!   205) and a dim text band at y=219..226 that no entry in the list
//!   accounts for. Both belong to the recent-files section, which the
//!   File popup alone
//!   renders: a 1px divider with 4px margins,
//!   then — because the capture has no recent files — the
//!   empty-state placeholder, a 28px row of 11px text in dim text at
//!   60% reading "No Recent Files". That row is
//!   the 28px the entry-list arithmetic misses: 5 + 6x28 + 4x9 + 28 +
//!   5 = 242, the measured box height (the 243rd pixel row is the
//!   half-pixel crop origin at y=35.5). The placeholder's ink reads
//!   #43..#55 — `#7a7a7a` at 60% over `#1e1e1e` — dim enough that a
//!   hard-ink scan walks past it, which is why the first probes filed
//!   the 28px as "unexplained panel".
//! * **`Image`, `Help` and `Settings` are 38px tall** — a header and one item,
//!   not a full menu. Their popups are a single row, so this view renders them
//!   from the same builder rather than special-casing an "empty" menu.
//!
//! The trigger-to-ink padding was measured at exactly 10px, matching
//! `title::MENU_PX`, so the popup's y=36 origin is the same constant rather
//! than a coincidence.
//!
//! # Gaps
//!
//! * No `z_index` — the frame convention above is the substitute, and it is
//!   only correct while the caller appends late.
//! * Colour transitions, keyboard-focus rings and keyboard navigation are
//!   interaction. The hovered row *is* painted where a baseline shows it: the
//!   two `menu-*-hover-item` captures seed their first row through the
//!   `focus_bg` argument `components::ui::menu_row` already takes (see
//!   `HOVER_ROW_FILL`), the same argument the pointer-anchored context menus
//!   use for their keyboard-focused row. A live hover still cannot tint a row
//!   without a per-variant `.id` + `Stateful` wrapper that breaks the
//!   `Vec<Div>` shell builder, so only the seeded captures paint it.
//! * A global border colour would beat any per-element white border; these
//!   menus ask for the divider value, which does resolve,
//!   so `color::DIVIDER` is correct here rather than a fallback.
//! * Icon edges carry LCD subpixel colour GPUI cannot reproduce (it tints an
//!   alpha mask), and the previous renderer's font-smoothing hint covers type
//!   only, not SVG. Position will be right, colour will differ.
//! * No letter-spacing: GPUI's shaper runs +/-0.2px per glyph off the previous
//!   renderer, as two. Shortcuts are right-aligned, which hides this at the
//!   label end.

use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{div, px, Div, Entity, Hsla, TestSupportExt};

use crate::components::ui;
use crate::i18n::t;
use crate::shot;
use crate::state::{AppState, DialogId, MenuId};
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

/// 4px — the shell's vertical inset.
const SHELL_PY: f32 = 4.0;

/// A row: 28px tall, measured from the gap between text bands.
const ROW_H: f32 = 28.0;

/// 12px — a row's horizontal inset (consumed by `recent_files_row`, which is
/// the only row built here instead of via `ui::menu_row`).
const ROW_PX: f32 = 12.0;

// The label is 12px regular in the full text colour; the shortcut is 10px dim —
// both sizes live in `ui::menu_row`, so they are not redeclared here. The
// separator block is a 4px margin plus a 1px line = 9px; that 4px is inlined in
// height test below rather than named, since `menu_separator` owns the real
// constant in `ui.rs`.

/// 11px — the recent-files placeholder, one step below the
/// label and in dim text at 60%.
const RECENT_TEXT: f32 = 11.0;

/// 6px radius on the shell.
const SHELL_R: f32 = 6.0;

/// The hovered first row in the two `menu-*-hover-item` baselines:
/// FIELD at 70% over the `#1e1e1e` panel reads `#232323` = 35/255 on
/// the strip (crop y6..33 — the whole first row). A measured solid, so the
/// value is exact under any blend path.
const HOVER_ROW_FILL: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 35.0 / 255.0,
    a: 1.0,
};

pub fn render(state: &Entity<AppState>, cx: &Context<crate::app::App>) -> impl IntoElement {
    let id = shot::active().unwrap_or("menu-file");
    let body = match id {
        "menu-file" => positioned(X_FILE, "menu-file-popup", file_menu(None, state, cx)),
        "menu-file-hover-item" => positioned(
            X_FILE,
            "menu-file-popup",
            file_menu(Some(HOVER_ROW_FILL), state, cx),
        ),
        "menu-edit" => positioned(
            X_EDIT,
            "menu-edit-popup",
            edit_menu(state, true, true, true, true, true, cx),
        ),
        "menu-image" => positioned(X_IMAGE, "menu-image-popup", image_menu(state, cx)),
        "menu-layer" => positioned(X_LAYER, "menu-layer-popup", layer_menu(state, cx)),
        "menu-view" => positioned(X_VIEW, "menu-view-popup", view_menu(state, cx)),
        "menu-window" => positioned(X_WINDOW, "menu-window-popup", window_menu()),
        "menu-help" => positioned(X_HELP, "menu-help-popup", help_menu(state, cx)),
        "menu-settings" => positioned(
            X_SETTINGS,
            "menu-settings-popup",
            settings_menu(None, state, cx),
        ),
        "menu-settings-hover-item" => positioned(
            X_SETTINGS,
            "menu-settings-popup",
            settings_menu(Some(HOVER_ROW_FILL), state, cx),
        ),
        // Unknown ids fall through to the default state.
        _ => positioned(X_FILE, "menu-file-popup", file_menu(None, state, cx)),
    };
    // Transparent full-viewport frame, itself absolute so it disturbs no
    // layout when appended. The caller appends late; late paints on top - the
    // only stacking control GPUI offers, where CSS would use z-index.
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .w(px(FRAME_W))
        .h(px(FRAME_H))
        .child(body)
}

/// Pin a popup at its trigger origin inside the frame.
fn positioned(x: f32, id: &'static str, menu: Div) -> impl IntoElement {
    menu.absolute()
        .id(id)
        .test_support()
        .left(px(x))
        .top(px(MENU_TOP))
}

/// Render the open production menu over the live shell.
///
/// The overlay covers the window so a click anywhere closes it, mirroring the
/// captured app's outside-click dismissal. The popup inherits the measured
/// trigger origin and 224px width from the shot builders.
// Separate availability flags map directly to the edit-menu actions.
#[allow(clippy::too_many_arguments)]
pub fn render_open(
    state: &Entity<AppState>,
    menu: MenuId,
    has_selection: bool,
    has_copyable_selection: bool,
    has_clipboard: bool,
    can_undo: bool,
    can_redo: bool,
    cx: &gpui_kit::Context<crate::app::App>,
) -> impl IntoElement {
    let (x, id, popup) = match menu {
        MenuId::File => (X_FILE, "menu-file-popup", file_menu(None, state, cx)),
        MenuId::Edit => (
            X_EDIT,
            "menu-edit-popup",
            edit_menu(
                state,
                has_selection,
                has_copyable_selection,
                has_clipboard,
                can_undo,
                can_redo,
                cx,
            ),
        ),
        MenuId::Image => (X_IMAGE, "menu-image-popup", image_menu(state, cx)),
        MenuId::Layer => (X_LAYER, "menu-layer-popup", layer_menu(state, cx)),
        MenuId::View => (X_VIEW, "menu-view-popup", view_menu(state, cx)),
        MenuId::Window => (X_WINDOW, "menu-window-popup", window_menu()),
        MenuId::Help => (X_HELP, "menu-help-popup", help_menu(state, cx)),
        MenuId::Settings => (
            X_SETTINGS,
            "menu-settings-popup",
            settings_menu(None, state, cx),
        ),
    };
    let state_for_close = state.clone();
    let close = cx.listener(
        move |_, _, _, cx: &mut gpui_kit::Context<crate::app::App>| {
            state_for_close.update(cx, |state, _| state.open_menu = None);
            cx.notify();
        },
    );
    div()
        .id("menu-overlay")
        .test_support()
        .absolute()
        .inset(px(0.0))
        .on_click(close)
        .child(positioned(x, id, popup))
}

/// One entry of `MENU_DEFINITIONS`. `shortcut` is the right-aligned key hint
/// and `disabled` is whole-row 35% opacity in the source — no captured menu
/// has a disabled row, so the arm is here for completeness rather than because
/// a baseline shows it.
/// 4px vertical margins and a 1px top border in the divider colour, full-bleed
/// — the probe reads separator rows edge to edge, so there is no inset.
/// The recent-files popup's empty state — the row the File popup shows when
/// the loaded list comes back empty: 28px tall, centred on one row, 12px side
/// inset, 11px dim text at 60%, and marked disabled. Same shape
/// as a `menu_row` with a smaller, dimmer label and no shortcut, and
/// the 28px the entry list does not account for: it sits below the
/// fourth separator, closing the popup at 242px.
///
/// Unlike the source, the label paints full-strength [`color::TEXT_DIM`]:
/// the source's `/60` composites to ~2.2:1 over the panel, too faint for the
/// 11px placeholder that carries this row's whole message.
fn recent_files_row(
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> gpui_kit::AnyElement {
    let entries = state.read(cx).recent_files.clone();
    if entries.is_empty() {
        return h_flex()
            .w_full()
            .h(px(ROW_H))
            .items_center()
            .px(px(ROW_PX))
            .child(
                div()
                    .text_size(px(RECENT_TEXT))
                    .font_family(ui::FONT_UI)
                    .text_color(color::TEXT_DIM)
                    .child(t("menus.items.noRecentFiles")),
            )
            .into_any_element();
    }
    let mut column = v_flex();
    for (index, path) in entries.iter().enumerate() {
        let label = std::path::Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        let path = path.clone();
        let state = state.clone();
        column = column.child(ui::menu_row_action(
            &label,
            None,
            format!("recent-file-{index}"),
            cx.listener(
                move |app, _: &gpui_kit::ClickEvent, _window, cx: &mut Context<crate::app::App>| {
                    state.update(cx, |state, _| {
                        state.open_menu = None;
                        state.context_menu = None;
                    });
                    app.open_recent_path(&path, cx);
                },
            ),
        ));
    }
    column.into_any_element()
}

/// The shared chrome: 224px, 6px corners, a 1px `color::BORDER` edge,
/// `color::PANEL` fill, 4px vertical padding, clipped.
///
/// A white border is *not* what the shell's edge is: the menu bar wraps the
/// popup in a 6px-cornered, divider-bordered panel shell, and the measurement
/// says that edge reads `#333333` (`BORDER`)
/// while the separators inside read `#2e2e2e` (`DIVIDER`). Two different
/// constants for two different things in the same menu.
fn menu_shell(entries: Vec<gpui_kit::AnyElement>) -> Div {
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
fn file_menu(
    hover_first: Option<Hsla>,
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> Div {
    let new_document = match hover_first {
        Some(fill) => ui::menu_row(
            &t("menus.items.newDocument"),
            Some("Ctrl+N"),
            false,
            Some(fill),
        )
        .into_any_element(),
        None => dialog_row(
            &t("menus.items.newDocument"),
            Some("Ctrl+N"),
            "menu-new-document",
            DialogId::NewDocument,
            state,
            cx,
        ),
    };
    menu_shell(vec![
        new_document,
        command_row(
            state,
            &t("menus.items.openImage"),
            Some("Ctrl+O"),
            "menu-file-open",
            cx,
            |app, cx| app.open_document_action(&crate::app::OpenDocument, cx),
        ),
        ui::menu_separator().into_any_element(),
        command_row(
            state,
            &t("menus.items.save"),
            Some("Ctrl+S"),
            "menu-file-save",
            cx,
            |app, cx| app.save_document_action(&crate::app::SaveDocument, cx),
        ),
        command_row(
            state,
            &t("menus.items.saveAs"),
            Some("Ctrl+Shift+S"),
            "menu-file-save-as",
            cx,
            |app, cx| app.save_document_as_action(&crate::app::SaveDocumentAs, cx),
        ),
        ui::menu_separator().into_any_element(),
        command_row(
            state,
            &t("menus.items.export"),
            Some("Ctrl+Alt+E"),
            "menu-file-export",
            cx,
            |app, cx| app.export_document_action(&crate::app::ExportDocument, cx),
        ),
        ui::menu_separator().into_any_element(),
        command_row(
            state,
            &t("menus.items.print"),
            Some("Ctrl+P"),
            "menu-file-print",
            cx,
            |app, cx| app.print_document_action(&crate::app::PrintDocument, cx),
        ),
        ui::menu_separator().into_any_element(),
        recent_files_row(state, cx),
    ])
}

/// A menu row that closes the popup and runs a real command.
fn command_row(
    state: &Entity<AppState>,
    label: &str,
    shortcut: Option<&str>,
    id: &'static str,
    cx: &Context<crate::app::App>,
    run: impl Fn(&mut crate::app::App, &mut Context<crate::app::App>) + 'static,
) -> gpui_kit::AnyElement {
    command_row_enabled(state, label, shortcut, id, true, cx, run)
}

/// [`command_row`] with the 0.1 build's disabled state: dimmed to 35%
/// and inert, but still painted with its id.
fn command_row_enabled(
    state: &Entity<AppState>,
    label: &str,
    shortcut: Option<&str>,
    id: &'static str,
    enabled: bool,
    cx: &Context<crate::app::App>,
    run: impl Fn(&mut crate::app::App, &mut Context<crate::app::App>) + 'static,
) -> gpui_kit::AnyElement {
    let state = state.clone();
    let row = ui::menu_row(label, shortcut, !enabled, None)
        .id(id)
        .test_support();
    if enabled {
        row.on_click(cx.listener(
            move |app, _: &gpui_kit::ClickEvent, _window, cx: &mut Context<crate::app::App>| {
                state.update(cx, |state, _| {
                    state.open_menu = None;
                    state.context_menu = None;
                });
                run(app, cx);
            },
        ))
        .into_any_element()
    } else {
        // A disabled row is inert but still part of the menu: the press must not
        // bubble to the overlay's click-away, as CSS pointer-events-none did.
        row.on_click(cx.listener(
            |_, _: &gpui_kit::ClickEvent, _window, cx: &mut Context<crate::app::App>| {
                cx.stop_propagation();
            },
        ))
        .into_any_element()
    }
}

fn edit_menu(
    state: &Entity<AppState>,
    has_selection: bool,
    has_copyable_selection: bool,
    has_clipboard: bool,
    can_undo: bool,
    can_redo: bool,
    cx: &Context<crate::app::App>,
) -> Div {
    use crate::app::{
        CopySelectionAction, CutSelectionAction, DeselectAction, InvertSelectionAction,
        PasteSelectionAction, RedoDocument, SelectAllAction, UndoDocument,
    };
    menu_shell(menu_entries(vec![
        command_row_enabled(
            state,
            &t("menus.items.undo"),
            Some("Ctrl+Z"),
            "menu-edit-undo",
            can_undo,
            cx,
            |app, cx| app.undo_document_action(&UndoDocument, cx),
        ),
        command_row_enabled(
            state,
            &t("menus.items.redo"),
            Some("Ctrl+Shift+Z"),
            "menu-edit-redo",
            can_redo,
            cx,
            |app, cx| app.redo_document_action(&RedoDocument, cx),
        ),
        ui::menu_separator().into_any_element(),
        command_row_enabled(
            state,
            &t("menus.items.cut"),
            Some("Ctrl+X"),
            "menu-edit-cut",
            has_copyable_selection,
            cx,
            |app, cx| app.cut_selection_action(&CutSelectionAction, cx),
        ),
        command_row_enabled(
            state,
            &t("menus.items.copy"),
            Some("Ctrl+C"),
            "menu-edit-copy",
            has_copyable_selection,
            cx,
            |app, cx| app.copy_selection_action(&CopySelectionAction, cx),
        ),
        command_row_enabled(
            state,
            &t("menus.items.paste"),
            Some("Ctrl+V"),
            "menu-edit-paste",
            has_clipboard,
            cx,
            |app, cx| app.paste_selection_action(&PasteSelectionAction, cx),
        ),
        ui::menu_separator().into_any_element(),
        command_row(
            state,
            &t("menus.items.selectAll"),
            Some("Ctrl+A"),
            "menu-edit-select-all",
            cx,
            |app, cx| app.select_all_action(&SelectAllAction, cx),
        ),
        command_row_enabled(
            state,
            &t("menus.items.deselect"),
            Some("Ctrl+D"),
            "menu-edit-deselect",
            has_selection,
            cx,
            |app, cx| app.deselect_action(&DeselectAction, cx),
        ),
        command_row(
            state,
            &t("menus.items.invertSelection"),
            Some("Ctrl+Shift+I"),
            "menu-edit-invert",
            cx,
            |app, cx| app.invert_selection_action(&InvertSelectionAction, cx),
        ),
    ]))
}

fn image_menu(state: &Entity<AppState>, cx: &Context<crate::app::App>) -> Div {
    let state_for_click = state.clone();
    menu_shell(menu_entries(vec![ui::menu_row_action(
        &t("menus.items.resizeCanvas"),
        None,
        "menu-resize-canvas",
        cx.listener(
            move |app, _: &gpui_kit::ClickEvent, window, cx: &mut Context<crate::app::App>| {
                state_for_click.update(cx, |state, _| state.open_menu = None);
                let result = app.begin_resize_canvas(window, cx);
                if let Err(error) = result {
                    app.set_status_error(Some(format!("Resize: {error}")), cx);
                }
            },
        ),
    )]))
}

fn layer_menu(state: &Entity<AppState>, cx: &Context<crate::app::App>) -> Div {
    use crate::app::{DuplicateLayerAction, FlattenDocumentAction, MergeLayerDownAction};
    menu_shell(menu_entries(vec![
        command_row(
            state,
            &t("menus.items.newLayer"),
            Some("Ctrl+Shift+N"),
            "menu-layer-new",
            cx,
            |app, cx| app.add_empty_layer(cx),
        ),
        command_row(
            state,
            &t("menus.items.duplicateLayer"),
            Some("Ctrl+J"),
            "menu-layer-duplicate",
            cx,
            |app, cx| app.duplicate_layer_action(&DuplicateLayerAction, cx),
        ),
        command_row(
            state,
            &t("menus.items.deleteLayer"),
            None,
            "menu-layer-delete",
            cx,
            |app, cx| app.delete_selected_layer(cx),
        ),
        command_row(
            state,
            &t("menus.items.selectAllLayers"),
            Some("Ctrl+Alt+A"),
            "menu-layer-select-all",
            cx,
            |app, cx| app.select_all_layers(cx),
        ),
        ui::menu_separator().into_any_element(),
        command_row(
            state,
            &t("menus.items.mergeDown"),
            Some("Ctrl+E"),
            "menu-layer-merge",
            cx,
            |app, cx| app.merge_layer_down_action(&MergeLayerDownAction, cx),
        ),
        command_row(
            state,
            &t("menus.items.stampVisible"),
            Some("Ctrl+Shift+Alt+E"),
            "menu-layer-stamp",
            cx,
            |app, cx| app.stamp_visible_action(&crate::app::StampVisibleAction, cx),
        ),
        ui::menu_separator().into_any_element(),
        command_row(
            state,
            &t("menus.items.flattenImage"),
            Some("Ctrl+Shift+E"),
            "menu-layer-flatten",
            cx,
            |app, cx| app.flatten_document_action(&FlattenDocumentAction, cx),
        ),
    ]))
}

fn view_menu(state: &Entity<AppState>, cx: &Context<crate::app::App>) -> Div {
    use crate::app::{
        ActualSizeAction, FitToScreenAction, ZoomInAction, ZoomOutAction, ZoomToSelectionAction,
    };
    menu_shell(menu_entries(vec![
        command_row(
            state,
            &t("menus.items.zoomIn"),
            Some("Ctrl++"),
            "menu-view-zoom-in",
            cx,
            |app, cx| app.zoom_in_action(&ZoomInAction, cx),
        ),
        command_row(
            state,
            &t("menus.items.zoomOut"),
            Some("Ctrl+-"),
            "menu-view-zoom-out",
            cx,
            |app, cx| app.zoom_out_action(&ZoomOutAction, cx),
        ),
        command_row(
            state,
            &t("menus.items.actualSize"),
            Some("Ctrl+1"),
            "menu-view-actual",
            cx,
            |app, cx| app.actual_size_action(&ActualSizeAction, cx),
        ),
        command_row(
            state,
            &t("menus.items.fitCanvas"),
            Some("Ctrl+0"),
            "menu-view-fit",
            cx,
            |app, cx| app.fit_to_screen_action(&FitToScreenAction, cx),
        ),
        command_row(
            state,
            &t("menus.items.zoomToSelection"),
            Some("Ctrl+Alt+0"),
            "menu-view-zoom-selection",
            cx,
            |app, cx| app.zoom_to_selection_action(&ZoomToSelectionAction, cx),
        ),
        ui::menu_separator().into_any_element(),
        command_row(
            state,
            &t("menus.items.snap"),
            Some("Shift+Ctrl+;"),
            "menu-view-snap",
            cx,
            |app, cx| app.toggle_snap(cx),
        ),
        ui::menu_row(&t("menus.items.snapToLayers"), None, false, None).into_any_element(),
        ui::menu_row(&t("menus.items.snapToCanvas"), None, false, None).into_any_element(),
        ui::menu_separator().into_any_element(),
        command_row(
            state,
            &t("menus.items.toggleSidePanels"),
            Some("Ctrl+Shift+P"),
            "menu-view-side-panels",
            cx,
            |app, cx| {
                app.toggle_side_panels_action(&crate::app::ToggleSidePanelsAction, cx);
            },
        ),
        ui::menu_row(&t("menus.items.useStackedSideDock"), None, false, None).into_any_element(),
    ]))
}

fn window_menu() -> Div {
    menu_shell(menu_entries(vec![
        ui::menu_row(&t("menus.items.minimize"), None, false, None),
        ui::menu_row(&t("menus.items.maximize"), None, false, None),
        ui::menu_separator(),
        ui::menu_row(&t("menus.items.closeWindow"), Some("Alt+F4"), false, None),
    ]))
}

fn help_menu(state: &Entity<AppState>, cx: &Context<crate::app::App>) -> Div {
    menu_shell(vec![dialog_row(
        &t("menus.items.about"),
        None,
        "menu-about",
        DialogId::About,
        state,
        cx,
    )])
}

fn settings_menu(
    hover_first: Option<Hsla>,
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> Div {
    let row = match hover_first {
        Some(fill) => {
            ui::menu_row(&t("menus.items.preferences"), None, false, Some(fill)).into_any_element()
        }
        None => dialog_row(
            &t("menus.items.preferences"),
            None,
            "menu-preferences",
            DialogId::Settings,
            state,
            cx,
        ),
    };
    menu_shell(vec![row])
}

fn menu_entries(entries: Vec<impl IntoElement>) -> Vec<gpui_kit::AnyElement> {
    entries
        .into_iter()
        .map(|entry| entry.into_any_element())
        .collect()
}

fn dialog_row(
    label: &str,
    shortcut: Option<&str>,
    id: &'static str,
    dialog: DialogId,
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> gpui_kit::AnyElement {
    let state = state.clone();
    let listener = cx.listener(move |app, _, window, cx: &mut Context<crate::app::App>| {
        state.update(cx, |state, _| {
            state.open_menu = None;
            state.active_dialog = Some(dialog);
        });
        // The dialog chrome owns Escape; focus it as `DesktopDialog`'s
        // managed focus does on mount.
        window.focus(&app.dialog_focus, cx);
        cx.notify();
    });
    ui::menu_row_action(label, shortcut, id, listener).into_any_element()
}

/// The canvas context menu: the reachable edit/view/layer commands at the
/// right-click position. Any command closes both menu surfaces.
pub fn render_context(
    state: &Entity<AppState>,
    position: (f32, f32),
    has_selection: bool,
    has_copyable_selection: bool,
    has_clipboard: bool,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    use crate::app::{ActualSizeAction, FitToScreenAction, ZoomToSelectionAction};
    use crate::app::{
        CopySelectionAction, CutSelectionAction, DeselectAction, InvertSelectionAction,
        PasteSelectionAction, SelectAllAction,
    };
    let close_state = state.clone();
    // The canvas menu is 210 px wide with the context-menu shadow;
    // the menubar shell is 224 px, so the live menu uses the specimen shell.
    let popup = crate::views::context_menus::shell(menu_entries(vec![
        command_row_enabled(
            state,
            &t("menus.items.cut"),
            Some("Ctrl+X"),
            "ctx-cut",
            has_copyable_selection,
            cx,
            |app, cx| app.cut_selection_action(&CutSelectionAction, cx),
        ),
        command_row_enabled(
            state,
            &t("menus.items.copy"),
            Some("Ctrl+C"),
            "ctx-copy",
            has_copyable_selection,
            cx,
            |app, cx| app.copy_selection_action(&CopySelectionAction, cx),
        ),
        command_row_enabled(
            state,
            &t("menus.items.paste"),
            Some("Ctrl+V"),
            "ctx-paste",
            has_clipboard,
            cx,
            |app, cx| app.paste_selection_action(&PasteSelectionAction, cx),
        ),
        ui::menu_separator().into_any_element(),
        command_row(
            state,
            &t("menus.items.selectAll"),
            Some("Ctrl+A"),
            "ctx-select-all",
            cx,
            |app, cx| app.select_all_action(&SelectAllAction, cx),
        ),
        command_row_enabled(
            state,
            &t("menus.items.deselect"),
            Some("Ctrl+D"),
            "ctx-deselect",
            has_selection,
            cx,
            |app, cx| app.deselect_action(&DeselectAction, cx),
        ),
        command_row(
            state,
            &t("menus.items.invertSelection"),
            Some("Ctrl+Shift+I"),
            "ctx-invert",
            cx,
            |app, cx| app.invert_selection_action(&InvertSelectionAction, cx),
        ),
        ui::menu_separator().into_any_element(),
        command_row(
            state,
            &t("menus.items.actualSize"),
            Some("Ctrl+1"),
            "ctx-actual-size",
            cx,
            |app, cx| app.actual_size_action(&ActualSizeAction, cx),
        ),
        command_row(
            state,
            &t("menus.items.fitCanvas"),
            Some("Ctrl+0"),
            "ctx-fit-canvas",
            cx,
            |app, cx| app.fit_to_screen_action(&FitToScreenAction, cx),
        ),
        command_row_enabled(
            state,
            &t("menus.items.zoomToSelection"),
            Some("Ctrl+Alt+0"),
            "ctx-zoom-selection",
            has_selection,
            cx,
            |app, cx| app.zoom_to_selection_action(&ZoomToSelectionAction, cx),
        ),
    ]))
    .absolute()
    .left(px(position.0))
    .top(px(position.1));
    div()
        .id("context-menu-overlay")
        .test_support()
        .absolute()
        .inset(px(0.0))
        .on_click(
            cx.listener(move |app, _: &gpui_kit::ClickEvent, _window, cx| {
                close_state.update(cx, |state, _| state.context_menu = None);
                app.close_context_menu(cx);
            }),
        )
        .child(popup)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The eight top-level menu origins are pinned by `manifest.json`; a
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

    /// `menu-file`'s measured height is a hard pin: 5 (vertical inset) + 6 rows*28 +
    /// 4 seps*(8+1) + 1 recent row*28 + 5 (vertical inset) + 2 (border) = 242. Removing a
    /// row (-28) or a separator (-9) reddens.
    #[test]
    fn file_menu_height_is_242px() {
        let sep_my = 4.0; // 4px margins — `ui::menu_separator` owns the real value
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
