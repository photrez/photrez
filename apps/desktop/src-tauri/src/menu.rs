// SPDX-License-Identifier: AGPL-3.0-or-later
// â”€â”€â”€ Native Application Menu â”€â”€â”€
//
// Builds the native OS menu bar and defines which menu IDs are
// forwarded to the frontend as `photrez://native-menu` events.

use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    Manager, Runtime,
};

pub(crate) const NATIVE_MENU_EVENT: &str = "photrez://native-menu";

pub(crate) const EDITOR_MENU_IDS: &[&str] = &[
    "file.new",
    "file.open",
    "file.print",
    "file.export",
    "edit.undo",
    "edit.redo",
    "image.resize",
    "layer.new",
    "layer.duplicate",
    "layer.delete",
    "layer.merge-down",
    "layer.flatten",
    "view.zoom-in",
    "view.zoom-out",
    "view.actual-size",
    "view.fit-canvas",
    "view.toggle-side-panels",
];

pub(crate) fn build_native_menu<R: Runtime, M: Manager<R>>(
    manager: &M,
) -> tauri::Result<Menu<R>> {
    let new_document = MenuItemBuilder::with_id("file.new", "&New Document")
        .accelerator("CmdOrCtrl+N")
        .build(manager)?;
    let open = MenuItemBuilder::with_id("file.open", "&Open Image…")
        .accelerator("CmdOrCtrl+O")
        .build(manager)?;
    let print = MenuItemBuilder::with_id("file.print", "&Print...")
        .accelerator("CmdOrCtrl+P")
        .build(manager)?;
    let export = MenuItemBuilder::with_id("file.export", "&Export...")
        .accelerator("CmdOrCtrl+S")
        .build(manager)?;
    let undo = MenuItemBuilder::with_id("edit.undo", "&Undo")
        .accelerator("CmdOrCtrl+Z")
        .build(manager)?;
    let redo = MenuItemBuilder::with_id("edit.redo", "&Redo")
        .accelerator("CmdOrCtrl+Shift+Z")
        .build(manager)?;
    let resize = MenuItemBuilder::with_id("image.resize", "Resize &Canvasâ€¦")
        .build(manager)?;
    let new_layer = MenuItemBuilder::with_id("layer.new", "&New Layer")
        .accelerator("CmdOrCtrl+Shift+N")
        .build(manager)?;
    let duplicate_layer = MenuItemBuilder::with_id("layer.duplicate", "&Duplicate Layer")
        .accelerator("CmdOrCtrl+J")
        .build(manager)?;
    let delete_layer = MenuItemBuilder::with_id("layer.delete", "&Delete Layer")
        .build(manager)?;
    let merge_down = MenuItemBuilder::with_id("layer.merge-down", "&Merge Down")
        .accelerator("CmdOrCtrl+E")
        .build(manager)?;
    let flatten_image = MenuItemBuilder::with_id("layer.flatten", "&Flatten Image")
        .accelerator("CmdOrCtrl+Shift+E")
        .build(manager)?;
    let toggle_side_panels =
        MenuItemBuilder::with_id("view.toggle-side-panels", "Toggle Side &Panels")
            .accelerator("CmdOrCtrl+Shift+P")
            .build(manager)?;
    let zoom_in = MenuItemBuilder::with_id("view.zoom-in", "Zoom &In")
        .accelerator("CmdOrCtrl+=")
        .build(manager)?;
    let zoom_out = MenuItemBuilder::with_id("view.zoom-out", "Zoom &Out")
        .accelerator("CmdOrCtrl+-")
        .build(manager)?;
    let actual_size = MenuItemBuilder::with_id("view.actual-size", "&Actual Size")
        .accelerator("CmdOrCtrl+1")
        .build(manager)?;
    let fit_canvas = MenuItemBuilder::with_id("view.fit-canvas", "&Fit Canvas")
        .accelerator("CmdOrCtrl+0")
        .build(manager)?;

    let file_menu = SubmenuBuilder::new(manager, "&File")
        .item(&new_document)
        .item(&open)
        .item(&print)
        .separator()
        .item(&export)
        .separator()
        .quit()
        .build()?;
    // Tauri's predefined Undo/Redo items are unsupported on Windows, so these
    // two editor mutations use custom IDs while text editing keeps native items.
    let edit_menu = SubmenuBuilder::new(manager, "&Edit")
        .item(&undo)
        .item(&redo)
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let image_menu = SubmenuBuilder::new(manager, "&Image")
        .item(&resize)
        .build()?;
    let layer_menu = SubmenuBuilder::new(manager, "&Layer")
        .item(&new_layer)
        .item(&duplicate_layer)
        .item(&delete_layer)
        .separator()
        .item(&merge_down)
        .item(&flatten_image)
        .build()?;
    let view_menu = SubmenuBuilder::new(manager, "&View")
        .item(&zoom_in)
        .item(&zoom_out)
        .item(&actual_size)
        .item(&fit_canvas)
        .separator()
        .item(&toggle_side_panels)
        .build()?;
    let window_menu = SubmenuBuilder::new(manager, "&Window")
        .minimize()
        .maximize()
        .separator()
        .close_window()
        .build()?;
    let help_menu = SubmenuBuilder::new(manager, "&Help")
        .about(None)
        .build()?;

    MenuBuilder::new(manager)
        .items(&[
            &file_menu,
            &edit_menu,
            &image_menu,
            &layer_menu,
            &view_menu,
            &window_menu,
            &help_menu,
        ])
        .build()
}

pub(crate) fn is_editor_menu_id(id: &str) -> bool {
    EDITOR_MENU_IDS.contains(&id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_menu_builds_with_all_editor_commands() {
        fn contains_id<R: tauri::Runtime>(
            items: &[tauri::menu::MenuItemKind<R>],
            expected: &str,
        ) -> bool {
            items.iter().any(|item| {
                item.id().0 == expected
                    || item.as_submenu().is_some_and(|submenu| {
                        submenu
                            .items()
                            .is_ok_and(|children| contains_id(&children, expected))
                    })
            })
        }

        let app = tauri::test::mock_app();
        let menu = build_native_menu(&app).expect("native menu should build");
        let items = menu.items().expect("top-level menu items");

        assert_eq!(items.len(), 7);
        for id in EDITOR_MENU_IDS {
            assert!(contains_id(&items, id), "native menu should contain {id}");
        }
    }

    #[test]
    fn test_only_known_editor_menu_ids_are_forwarded() {
        for id in EDITOR_MENU_IDS {
            assert!(is_editor_menu_id(id));
        }
        assert!(!is_editor_menu_id("Quit"));
        assert!(!is_editor_menu_id("unknown.command"));
    }
}
