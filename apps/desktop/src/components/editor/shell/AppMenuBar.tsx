import { For, Show, createSignal, createMemo, onCleanup, onMount } from "solid-js";
import { Portal } from "solid-js/web";
import { MENU_ITEMS } from "../editorData";
import type { MenuItem } from "../types";
import type { EditorCommand } from "../useEditorCommands";
import { useEditor } from "./EditorContext";
import { useI18n } from "@/i18n/I18nProvider";
import { getRecentFiles, clearRecentFiles, type RecentFile } from "@/lib/recentFiles";

type MenuEntry =
  | { kind: "item"; label: string; command: EditorCommand; shortcut?: string }
  | { kind: "separator" };

const MENU_LABEL_KEYS: Record<MenuItem, string> = {
  File: "menus.file",
  Edit: "menus.edit",
  Image: "menus.image",
  Layer: "menus.layer",
  View: "menus.view",
  Window: "menus.window",
  Help: "menus.help",
  Settings: "menus.settings",
};

const MENU_ITEM_KEYS: Partial<Record<EditorCommand, string>> = {
  "file.new": "menus.items.newDocument",
  "file.open": "menus.items.openImage",
  "file.save": "menus.items.save",
  "file.save-as": "menus.items.saveAs",
  "file.export": "menus.items.export",
  "file.print": "menus.items.print",
  "edit.undo": "menus.items.undo",
  "edit.redo": "menus.items.redo",
  "edit.cut": "menus.items.cut",
  "edit.copy": "menus.items.copy",
  "edit.paste": "menus.items.paste",
  "edit.select-all": "menus.items.selectAll",
  "edit.deselect": "menus.items.deselect",
  "edit.invert-selection": "menus.items.invertSelection",
  "image.resize": "menus.items.resizeCanvas",
  "layer.new": "menus.items.newLayer",
  "layer.duplicate": "menus.items.duplicateLayer",
  "layer.delete": "menus.items.deleteLayer",
  "layer.select-all": "menus.items.selectAllLayers",
  "layer.merge-down": "menus.items.mergeDown",
  "layer.stamp-visible": "menus.items.stampVisible",
  "layer.flatten": "menus.items.flattenImage",
  "view.zoom-in": "menus.items.zoomIn",
  "view.zoom-out": "menus.items.zoomOut",
  "view.actual-size": "menus.items.actualSize",
  "view.fit-canvas": "menus.items.fitCanvas",
  "view.zoom-to-selection": "menus.items.zoomToSelection",
  "view.toggle-snap": "menus.items.snap",
  "view.toggle-snap-layers": "menus.items.snapToLayers",
  "view.toggle-snap-canvas": "menus.items.snapToCanvas",
  "view.toggle-side-panels": "menus.items.toggleSidePanels",
  "view.toggle-right-dock-layout": "menus.items.useStackedSideDock",
  "window.minimize": "menus.items.minimize",
  "window.toggle-maximize": "menus.items.maximize",
  "window.close": "menus.items.closeWindow",
  "help.about": "menus.items.about",
  "app.settings": "menus.items.preferences",
};

const MENU_DEFINITIONS: Record<MenuItem, readonly MenuEntry[]> = {
  File: [
    { kind: "item", label: "New Document", command: "file.new", shortcut: "Ctrl+N" },
    { kind: "item", label: "Open Image…", command: "file.open", shortcut: "Ctrl+O" },
    { kind: "separator" },
    { kind: "item", label: "Save", command: "file.save", shortcut: "Ctrl+S" },
    { kind: "item", label: "Save As…", command: "file.save-as", shortcut: "Ctrl+Shift+S" },
    { kind: "separator" },
    { kind: "item", label: "Export…", command: "file.export", shortcut: "Ctrl+Alt+E" },
    { kind: "separator" },
    { kind: "item", label: "Print…", command: "file.print", shortcut: "Ctrl+P" },
  ],
  Edit: [
    { kind: "item", label: "Undo", command: "edit.undo", shortcut: "Ctrl+Z" },
    { kind: "item", label: "Redo", command: "edit.redo", shortcut: "Ctrl+Shift+Z" },
    { kind: "separator" },
    { kind: "item", label: "Cut", command: "edit.cut", shortcut: "Ctrl+X" },
    { kind: "item", label: "Copy", command: "edit.copy", shortcut: "Ctrl+C" },
    { kind: "item", label: "Paste", command: "edit.paste", shortcut: "Ctrl+V" },
    { kind: "separator" },
    { kind: "item", label: "Select All", command: "edit.select-all", shortcut: "Ctrl+A" },
    { kind: "item", label: "Deselect", command: "edit.deselect", shortcut: "Ctrl+D" },
    { kind: "item", label: "Invert Selection", command: "edit.invert-selection", shortcut: "Ctrl+Shift+I" },
  ],
  Image: [
    { kind: "item", label: "Resize Canvas…", command: "image.resize" },
  ],
  Layer: [
    { kind: "item", label: "New Layer", command: "layer.new", shortcut: "Ctrl+Shift+N" },
    { kind: "item", label: "Duplicate Layer", command: "layer.duplicate", shortcut: "Ctrl+J" },
    { kind: "item", label: "Delete Layer", command: "layer.delete" },
    { kind: "item", label: "Select All Layers", command: "layer.select-all", shortcut: "Ctrl+Alt+A" },
    { kind: "separator" },
    { kind: "item", label: "Merge Down", command: "layer.merge-down", shortcut: "Ctrl+E" },
    { kind: "item", label: "Stamp Visible", command: "layer.stamp-visible", shortcut: "Ctrl+Shift+Alt+E" },
    { kind: "separator" },
    { kind: "item", label: "Flatten Image", command: "layer.flatten", shortcut: "Ctrl+Shift+E" },
  ],
  View: [
    { kind: "item", label: "Zoom In", command: "view.zoom-in", shortcut: "Ctrl++" },
    { kind: "item", label: "Zoom Out", command: "view.zoom-out", shortcut: "Ctrl+-" },
    { kind: "item", label: "Actual Size", command: "view.actual-size", shortcut: "Ctrl+1" },
    { kind: "item", label: "Fit Canvas", command: "view.fit-canvas", shortcut: "Ctrl+0" },
    { kind: "item", label: "Zoom to Selection", command: "view.zoom-to-selection", shortcut: "Ctrl+Alt+0" },
    { kind: "separator" },
    { kind: "item", label: "Snap", command: "view.toggle-snap", shortcut: "Shift+Ctrl+;" },
    { kind: "item", label: "Snap to Layers", command: "view.toggle-snap-layers" },
    { kind: "item", label: "Snap to Canvas", command: "view.toggle-snap-canvas" },
    { kind: "separator" },
    { kind: "item", label: "Toggle Side Panels", command: "view.toggle-side-panels", shortcut: "Ctrl+Shift+P" },
    { kind: "item", label: "Use Stacked Side Dock", command: "view.toggle-right-dock-layout" },
  ],
  Window: [
    { kind: "item", label: "Minimize", command: "window.minimize" },
    { kind: "item", label: "Maximize / Restore", command: "window.toggle-maximize" },
    { kind: "separator" },
    { kind: "item", label: "Close Window", command: "window.close", shortcut: "Alt+F4" },
  ],
  Help: [
    { kind: "item", label: "About Photrez", command: "help.about" },
  ],
  Settings: [
    { kind: "item", label: "Preferences…", command: "app.settings" },
  ],
};

type AppMenuBarProps = {
  execute: (command: EditorCommand) => void;
  isEnabled: (command: EditorCommand) => boolean;
  isRightDockOpen: boolean;
  onOpenRecent?: (path: string) => void;
  onClearRecent?: () => void;
};

function RecentFilesMenu(props: {
  onOpenRecent: (path: string) => void;
  onClearRecent: () => void;
}) {
  const { t } = useI18n();
  const recent = getRecentFiles();
  return (
    <>
      <Show when={recent.length > 0}>
        <div role="separator" class="my-1 h-px bg-editor-divider" />
        <For each={recent}>
          {(file: RecentFile) => (
            <button
              type="button"
              role="menuitem"
              aria-label={`Open ${file.name}`}
              class="flex h-7 w-full items-center gap-3 px-3 text-left outline-none hover:bg-editor-field/70 focus-visible:bg-editor-field/70"
              onClick={() => props.onOpenRecent(file.path)}
            >
              <span class="flex-1 truncate text-[12px]">{file.name}</span>
            </button>
          )}
        </For>
        <button
          type="button"
          role="menuitem"
          aria-label={t("menus.items.clearRecent", "Clear Recent Files")}
          class="flex h-7 w-full items-center px-3 text-left text-[11px] text-editor-text-dim outline-none hover:bg-editor-field/70 focus-visible:bg-editor-field/70"
          onClick={() => props.onClearRecent()}
        >
          {t("menus.items.clearRecent", "Clear Recent Files")}
        </button>
      </Show>
      <Show when={recent.length === 0}>
        <div role="separator" class="my-1 h-px bg-editor-divider" />
        <div role="menuitem" class="flex h-7 items-center px-3 text-[11px] text-editor-text-dim/60" aria-disabled="true">
          {t("menus.items.noRecentFiles", "No Recent Files")}
        </div>
      </Show>
    </>
  );
}

export function AppMenuBar(props: AppMenuBarProps) {
  const { t } = useI18n();
  const [openMenu, setOpenMenu] = createSignal<MenuItem | null>(null);
  const triggerRefs = new Map<MenuItem, HTMLButtonElement>();
  let navRef!: HTMLElement;

  const menuItems = (menu: MenuItem): HTMLButtonElement[] => {
    const popup = document.getElementById(`app-menu-${menu.toLowerCase()}`);
    return Array.from(popup?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? []);
  };

  const focusItem = (menu: MenuItem, index: number) => {
    const items = menuItems(menu);
    if (items.length === 0) return;
    items[(index + items.length) % items.length].focus();
  };

  const open = (menu: MenuItem, focusFirst = false) => {
    setOpenMenu(menu);
    if (focusFirst) queueMicrotask(() => focusItem(menu, 0));
  };

  const close = (restoreFocus = false) => {
    const current = openMenu();
    setOpenMenu(null);
    if (restoreFocus && current) queueMicrotask(() => triggerRefs.get(current)?.focus());
  };

  const adjacentMenu = (menu: MenuItem, direction: -1 | 1) => {
    const currentIndex = MENU_ITEMS.indexOf(menu);
    const next = MENU_ITEMS[(currentIndex + direction + MENU_ITEMS.length) % MENU_ITEMS.length];
    open(next, true);
  };

  const handleTriggerKeyDown = (event: KeyboardEvent, menu: MenuItem) => {
    if (event.key === "ArrowDown" || event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      open(menu, true);
    } else if (event.key === "Escape" && openMenu()) {
      event.preventDefault();
      close(true);
    }
  };

  const handlePopupKeyDown = (event: KeyboardEvent, menu: MenuItem) => {
    const items = menuItems(menu);
    const index = items.indexOf(document.activeElement as HTMLButtonElement);

    if (event.key === "ArrowDown") {
      event.preventDefault();
      focusItem(menu, index + 1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      focusItem(menu, index - 1);
    } else if (event.key === "Home") {
      event.preventDefault();
      focusItem(menu, 0);
    } else if (event.key === "End") {
      event.preventDefault();
      focusItem(menu, items.length - 1);
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      adjacentMenu(menu, -1);
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      adjacentMenu(menu, 1);
    } else if (event.key === "Escape") {
      event.preventDefault();
      close(true);
    } else if (event.key === "Tab") {
      close();
    }
  };

  const labelFor = (entry: Extract<MenuEntry, { kind: "item" }>) => {
    if (entry.command === "view.toggle-side-panels") {
      return props.isRightDockOpen ? t("menus.items.hideSidePanels", "Hide Side Panels") : t("menus.items.showSidePanels", "Show Side Panels");
    }
    if (entry.command === "view.toggle-right-dock-layout") {
      const editor = useEditor();
      return editor.rightDockLayout() === "side-by-side"
        ? t("menus.items.useStackedSideDock", "Use Stacked Side Dock")
        : t("menus.items.useSideBySideSideDock", "Use Side-by-Side Side Dock");
    }
    if (entry.command === "view.toggle-snap") {
      try {
        const editor = useEditor();
        return editor.moveSnapEnabled() ? `✓ ${t("menus.items.snap", "Snap")}` : t("menus.items.snap", "Snap");
      } catch {
        return t("menus.items.snap", entry.label);
      }
    }
    if (entry.command === "view.toggle-snap-layers") {
      try {
        const editor = useEditor();
        return editor.snapToLayersEnabled() ? `✓ ${t("menus.items.snapToLayers", "Snap to Layers")}` : t("menus.items.snapToLayers", "Snap to Layers");
      } catch {
        return t("menus.items.snapToLayers", entry.label);
      }
    }
    if (entry.command === "view.toggle-snap-canvas") {
      try {
        const editor = useEditor();
        return editor.snapToCanvasEnabled() ? `✓ ${t("menus.items.snapToCanvas", "Snap to Canvas")}` : t("menus.items.snapToCanvas", "Snap to Canvas");
      } catch {
        return t("menus.items.snapToCanvas", entry.label);
      }
    }
    const key = MENU_ITEM_KEYS[entry.command];
    return key ? t(key, entry.label) : entry.label;
  };

  const activate = (command: EditorCommand) => {
    if (!props.isEnabled(command)) return;
    const current = openMenu();
    close();
    if (current) triggerRefs.get(current)?.focus();
    props.execute(command);
  };

  // Popup position derived from the trigger button's bounding rect
  const popupStyle = createMemo(() => {
    const menu = openMenu();
    if (!menu) return { left: "0px", top: "0px" };
    const btn = triggerRefs.get(menu);
    if (!btn) return { left: "0px", top: "0px" };
    const rect = btn.getBoundingClientRect();
    return { left: `${rect.left}px`, top: `${rect.bottom}px` };
  });

  let popupRef!: HTMLDivElement;

  onMount(() => {
    const handlePointerDown = (event: PointerEvent) => {
      if (!openMenu()) return;
      const target = event.target as Node;
      // Click inside nav (trigger buttons) → let the button handle it, don't close
      if (navRef.contains(target)) return;
      // Click inside the popup (via Portal at body) → let it handle, don't close
      if (popupRef?.contains(target)) return;
      close();
    };
    document.addEventListener("pointerdown", handlePointerDown);
    onCleanup(() => document.removeEventListener("pointerdown", handlePointerDown));
  });

  return (
    <nav ref={navRef} class="hidden h-full items-center gap-0.5 md:flex" aria-label="Application menu">
      <For each={MENU_ITEMS}>
        {(menu) => (
          <div class="relative flex h-full items-center">
            <button
              ref={(element) => triggerRefs.set(menu, element)}
              type="button"
              class={`flex h-[26px] items-center justify-center rounded-[4px] px-2.5 text-[12px] font-medium tracking-wide transition-colors focus-visible:outline focus-visible:outline-1 focus-visible:outline-editor-accent ${
                openMenu() === menu
                  ? "bg-white/[0.08] text-editor-text-header font-semibold"
                  : "text-editor-text/90 hover:bg-white/[0.045] hover:text-editor-text"
              }`}
              aria-haspopup="menu"
              aria-expanded={openMenu() === menu}
              aria-controls={`app-menu-${menu.toLowerCase()}`}
              onClick={() => openMenu() === menu ? close() : open(menu)}
              onPointerEnter={() => {
                if (openMenu() && openMenu() !== menu) open(menu);
              }}
              onKeyDown={(event) => handleTriggerKeyDown(event, menu)}
            >
              {t(MENU_LABEL_KEYS[menu])}
            </button>
          </div>
        )}
      </For>

      {/* Portal popup — fixed positioning to escape WebGL compositing layer */}
      <Show when={openMenu()}>
        {(m) => (
          <Portal mount={document.body}>
            <div
              ref={popupRef}
              id={`app-menu-${m().toLowerCase()}`}
              role="menu"
              aria-label={`${t(MENU_LABEL_KEYS[m()])} menu`}
              class="fixed z-[100] min-w-56 rounded-[6px] border border-editor-divider bg-editor-panel py-1 text-[12px] text-editor-text shadow-xl"
              style={popupStyle()}
              onKeyDown={(event) => handlePopupKeyDown(event, m())}
            >
              <For each={MENU_DEFINITIONS[m()]}>
                {(entry) => (
                  <Show
                    when={entry.kind === "item" ? entry : null}
                    fallback={<div role="separator" class="my-1 h-px bg-editor-divider" />}
                  >
                    {(item) => (
                      <button
                        type="button"
                        role="menuitem"
                        aria-label={labelFor(item())}
                        disabled={!props.isEnabled(item().command)}
                        class="flex h-7 w-full items-center justify-between gap-6 px-3 text-left font-medium outline-none hover:bg-editor-field/70 focus-visible:bg-editor-field/70 disabled:text-editor-text-dim/45 disabled:hover:bg-transparent"
                        onClick={() => activate(item().command)}
                      >
                        <span class="text-[12px]">{labelFor(item())}</span>
                        <Show when={item().shortcut}>
                          <span class="text-[10.5px] font-normal tracking-tight text-editor-text-dim/80">{item().shortcut}</span>
                        </Show>
                      </button>
                    )}
                  </Show>
                )}
              </For>

              {/* Open Recent — only in File menu */}
              <Show when={m() === "File"}>
                <RecentFilesMenu
                  onOpenRecent={(path) => {
                    close();
                    props.onOpenRecent?.(path);
                  }}
                  onClearRecent={() => {
                    close();
                    clearRecentFiles();
                    props.onClearRecent?.();
                  }}
                />
              </Show>
            </div>
          </Portal>
        )}
      </Show>
    </nav>
  );
}
