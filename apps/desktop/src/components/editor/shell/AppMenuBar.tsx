import { For, Show, createSignal, createMemo, onCleanup, onMount } from "solid-js";
import { Portal } from "solid-js/web";
import { MENU_ITEMS } from "../editorData";
import type { MenuItem } from "../types";
import type { EditorCommand } from "../useEditorCommands";
import { useEditor } from "./EditorContext";
import { getRecentFiles, clearRecentFiles, type RecentFile } from "@/lib/recentFiles";

type MenuEntry =
  | { kind: "item"; label: string; command: EditorCommand; shortcut?: string }
  | { kind: "separator" };

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
          aria-label="Clear Recent Files"
          class="flex h-7 w-full items-center px-3 text-left text-[11px] text-editor-text-dim outline-none hover:bg-editor-field/70 focus-visible:bg-editor-field/70"
          onClick={() => props.onClearRecent()}
        >
          Clear Recent Files
        </button>
      </Show>
      <Show when={recent.length === 0}>
        <div role="separator" class="my-1 h-px bg-editor-divider" />
        <div role="menuitem" class="flex h-7 items-center px-3 text-[11px] text-editor-text-dim/60" aria-disabled="true">
          No Recent Files
        </div>
      </Show>
    </>
  );
}

export function AppMenuBar(props: AppMenuBarProps) {
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
      return props.isRightDockOpen ? "Hide Side Panels" : "Show Side Panels";
    }
    if (entry.command === "view.toggle-right-dock-layout") {
      const editor = useEditor();
      return editor.rightDockLayout() === "side-by-side" ? "Use Stacked Side Dock" : "Use Side-by-Side Side Dock";
    }
    if (entry.command === "view.toggle-snap") {
      try {
        const editor = useEditor();
        return editor.moveSnapEnabled() ? "✓ Snap" : "Snap";
      } catch {
        return entry.label;
      }
    }
    if (entry.command === "view.toggle-snap-layers") {
      try {
        const editor = useEditor();
        return editor.snapToLayersEnabled() ? "✓ Snap to Layers" : "Snap to Layers";
      } catch {
        return entry.label;
      }
    }
    if (entry.command === "view.toggle-snap-canvas") {
      try {
        const editor = useEditor();
        return editor.snapToCanvasEnabled() ? "✓ Snap to Canvas" : "Snap to Canvas";
      } catch {
        return entry.label;
      }
    }
    return entry.label;
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
              {menu}
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
              aria-label={`${m()} menu`}
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
