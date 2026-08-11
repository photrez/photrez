import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";
import { Icon } from "../icons";
import { isTauriRuntime, runTauriWindowAction } from "@/lib/desktop";
import { useEditor } from "./EditorContext";
import { useEditorCommands } from "../useEditorCommands";
import { AppMenuBar } from "./AppMenuBar";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openSingleFile } from "../editorOpenImage";
import { addRecentFile } from "@/lib/recentFiles";

type AppTitleBarProps = {
  isRightDockOpen: boolean;
  onToggleRightDock: () => void;
};

export function AppTitleBar(props: AppTitleBarProps) {
  const { activeDocumentId, documents, workspace, renderer, scheduler, showToast, setLoadingMessage } = useEditor();
  const commands = useEditorCommands(props.onToggleRightDock);

  const handleOpenRecent = (path: string) => {
    const name = path.split(/[/\\]/).pop() || path;
    openSingleFile(path, {
      workspace,
      renderer,
      scheduler,
      onError: (msg) => showToast(msg, "error"),
      onLoading: (msg) => setLoadingMessage(msg),
    }).then(() => {
      addRecentFile(path, name);
    }).catch((e) => {
      showToast(`Failed to open recent file: ${e instanceof Error ? e.message : String(e)}`, "error");
    });
  };

  const handleClearRecent = () => {
    // force re-render by toggling a reactive signal (not strictly needed
    // since the menu popup recreates on each open, but keeps code consistent)
  };

  const [isMaximized, setIsMaximized] = createSignal(false);

  onMount(() => {
    if (!isTauriRuntime()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    (async () => {
      if (disposed) return;
      try {
        const appWindow = getCurrentWindow();
        setIsMaximized(await appWindow.isMaximized());
        unlisten = await appWindow.onResized(async () => {
          if (disposed) return;
          try {
            setIsMaximized(await appWindow.isMaximized());
          } catch {
            // Permission may not be granted; treat as non-maximized.
            setIsMaximized(false);
          }
        });
      } catch {
        // Permission may not be granted; treat as non-maximized.
        setIsMaximized(false);
      }
    })();
    onCleanup(() => {
      disposed = true;
      unlisten?.();
    });
  });

  // ── System menu (Alt+Space) ──
  const [systemMenuOpen, setSystemMenuOpen] = createSignal(false);
  let systemMenuRef!: HTMLDivElement;

  // ── Keyboard accessibility: F10 (focus menu), Alt+Space (system menu) ──
  onMount(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Skip when focus is in an editable control
      const tag = document.activeElement?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

      // F10 without modifiers: focus first menu trigger
      if (e.key === "F10" && !e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        const firstMenu = document.querySelector<HTMLButtonElement>('button[aria-haspopup="menu"]');
        firstMenu?.focus();
        return;
      }

      // Alt+Space: toggle system menu
      if (e.key === " " && e.altKey && !e.ctrlKey && !e.shiftKey && !e.metaKey) {
        e.preventDefault();
        setSystemMenuOpen((prev) => !prev);
        return;
      }

      // Escape: close system menu
      if (e.key === "Escape" && systemMenuOpen()) {
        e.preventDefault();
        setSystemMenuOpen(false);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    onCleanup(() => window.removeEventListener("keydown", handleKeyDown));
  });

  // Close system menu on outside click
  onMount(() => {
    const handlePointerDown = (event: PointerEvent) => {
      if (!systemMenuOpen()) return;
      const target = event.target as Node;
      if (systemMenuRef?.contains(target)) return;
      setSystemMenuOpen(false);
    };
    document.addEventListener("pointerdown", handlePointerDown);
    onCleanup(() => document.removeEventListener("pointerdown", handlePointerDown));
  });

  const activeDocName = () => {
    const id = activeDocumentId();
    return documents().find((document) => document.id === id)?.displayName || "Untitled";
  };

  return (
    <header class="relative flex h-[46px] shrink-0 items-center border-b border-editor-divider bg-editor-topbar select-none overflow-hidden">
      <div class="flex min-w-0 items-center gap-2 self-stretch">
        <div
          class="flex h-full shrink-0 items-center pl-3 pr-2"
          data-tauri-drag-region
          onDblClick={() => runTauriWindowAction("toggleMaximize")}
        >
          <svg viewBox="0 0 512 512" class="size-[30px] shrink-0" aria-hidden="true">
            <defs>
              <linearGradient id="brandGradientTitle" x1="0%" y1="0%" x2="0%" y2="100%">
                <stop offset="0%" stop-color="#FFB31A" />
                <stop offset="100%" stop-color="var(--color-editor-accent)" />
              </linearGradient>
              <linearGradient id="bgDarkTitle" x1="0%" y1="0%" x2="0%" y2="100%">
                <stop offset="0%" stop-color="#2C2C2E" />
                <stop offset="100%" stop-color="#151516" />
              </linearGradient>
              <mask id="mountainMaskTitle" maskUnits="userSpaceOnUse" x="-500" y="-500" width="2000" height="2000">
                <rect x="-500" y="-500" width="2000" height="2000" fill="white" />
                <polygon points="250.4,320 300.86,127 307,127 357.6,320" fill="black" />
              </mask>
              <filter id="pShadowTitle" x="-20%" y="-20%" width="150%" height="150%">
                <feDropShadow dx="0" dy="16" stdDeviation="16" flood-color="#000000" flood-opacity="0.5" />
              </filter>
            </defs>
            <rect x="24" y="24" width="464" height="464" rx="100" fill="url(#bgDarkTitle)" />
            <g transform="translate(8, 48) scale(0.85)" filter="url(#pShadowTitle)">
              <path d="M 240 50 L 460 50 L 390 310 L 253 310 L 219 440 L 136 440 Z" fill="url(#brandGradientTitle)" mask="url(#mountainMaskTitle)" />
              <circle cx="333" cy="175" r="30" fill="#FFE57F" mask="url(#mountainMaskTitle)" />
            </g>
          </svg>
        </div>

        <AppMenuBar
          execute={commands.execute}
          isEnabled={commands.isEnabled}
          isRightDockOpen={props.isRightDockOpen}
          onOpenRecent={handleOpenRecent}
          onClearRecent={handleClearRecent}
        />
      </div>

      <div
        class="flex min-w-4 flex-1 items-center justify-center self-stretch"
        data-tauri-drag-region
        onDblClick={() => runTauriWindowAction("toggleMaximize")}
      >
        <div class="pointer-events-none flex items-center gap-1.5 opacity-90">
          <span class="text-[12px] font-semibold text-editor-text-header tracking-tight">
            {activeDocName()}
          </span>
          <span class="text-[11px] text-editor-text-dim/60">·</span>
          <span class="text-[11.5px] font-medium tracking-wider text-editor-text-dim/80">photrez</span>
        </div>
      </div>

      <div class="flex min-w-0 items-center">
        <div class="mr-3 flex shrink items-center gap-0.5 overflow-hidden text-editor-icon">
          <button
            onClick={commands.undo}
            class="flex size-7 items-center justify-center rounded-[4px] hover:bg-white/[0.045] hover:text-editor-text"
            aria-label="Undo"
          >
            <Icon name="undo" class="size-[16px]" strokeWidth={1.75} />
          </button>
          <button
            onClick={commands.redo}
            class="flex size-7 items-center justify-center rounded-[4px] hover:bg-white/[0.045] hover:text-editor-text"
            aria-label="Redo"
          >
            <Icon name="redo" class="size-[16px]" strokeWidth={1.75} />
          </button>
          <div class="mx-1 h-3.5 w-px bg-editor-divider" />
          <button
            class="flex size-7 items-center justify-center rounded-[4px] hover:bg-white/[0.045] hover:text-editor-text lg:hidden"
            aria-label={props.isRightDockOpen ? "Hide side panels" : "Show side panels"}
            onClick={props.onToggleRightDock}
          >
            <Icon
              name={props.isRightDockOpen ? "panel-right-close" : "panel-right-open"}
              class="size-[16px]"
              strokeWidth={1.75}
            />
          </button>
        </div>

        <div class="flex shrink-0 items-center pr-0 text-editor-icon">
          <button
            class="flex h-[46px] w-11 items-center justify-center hover:bg-white/[0.055] hover:text-editor-text"
            onClick={() => runTauriWindowAction("minimize")}
            aria-label="Minimize window"
          >
            <Icon name="minus" class="size-[15px]" strokeWidth={1.75} />
          </button>
          <button
            class="flex h-[46px] w-11 items-center justify-center hover:bg-white/[0.055] hover:text-editor-text"
            onClick={() => runTauriWindowAction("toggleMaximize")}
            aria-label={isMaximized() ? "Restore window" : "Maximize window"}
          >
            {isMaximized() ? (
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                <path d="M7.51758 5H6.00932C6.13697 3.32189 7.53905 2 9.24988 2H17.25C19.8733 2 22 4.12665 22 6.75V14.75C22 16.4608 20.6781 17.8629 19 17.9905V16.4823C19.8481 16.361 20.5 15.6316 20.5 14.75V6.75C20.5 4.95507 19.0449 3.5 17.25 3.5H9.24988C8.36825 3.5 7.63889 4.15193 7.51758 5ZM5.25003 6C3.45509 6 2 7.45507 2 9.25V18.75C2 20.5449 3.45509 22 5.25003 22H14.7501C16.5451 22 18.0002 20.5449 18.0002 18.75V9.25C18.0002 7.45507 16.5451 6 14.7501 6H5.25003ZM3.50001 9.25C3.50001 8.2835 4.28352 7.5 5.25003 7.5H14.7501C15.7166 7.5 16.5001 8.2835 16.5001 9.25V18.75C16.5001 19.7165 15.7166 20.5 14.7501 20.5H5.25003C4.28352 20.5 3.50001 19.7165 3.50001 18.75V9.25Z" fill="currentColor" />
              </svg>
            ) : (
              <Icon name="square" class="size-[12px]" strokeWidth={1.75} />
            )}
          </button>
          <button
            class="flex h-[46px] w-11 items-center justify-center hover:bg-red-500/85 hover:text-white"
            onClick={() => runTauriWindowAction("close")}
            aria-label="Close window"
          >
            <Icon name="x" class="size-[16px]" strokeWidth={1.75} />
          </button>
        </div>
      </div>

      {/* System menu (Alt+Space) */}
      <Show when={systemMenuOpen()}>
        <Portal mount={document.body}>
          <div
            ref={systemMenuRef}
            role="menu"
            aria-label="System menu"
            class="fixed z-[100] min-w-44 rounded-[6px] border border-editor-divider bg-editor-panel py-1 text-[12px] text-editor-text shadow-xl"
            style="left: 0; top: 46px;"
          >
            <button
              type="button"
              role="menuitem"
              class="flex h-7 w-full items-center px-3 text-left outline-none hover:bg-editor-field/70 focus-visible:bg-editor-field/70"
              onClick={() => { runTauriWindowAction("toggleMaximize"); setSystemMenuOpen(false); }}
              tabIndex={0}
            >
              {isMaximized() ? "Restore" : "Maximize"}
            </button>
            <button
              type="button"
              role="menuitem"
              class="flex h-7 w-full items-center px-3 text-left outline-none hover:bg-editor-field/70 focus-visible:bg-editor-field/70"
              onClick={() => { runTauriWindowAction("minimize"); setSystemMenuOpen(false); }}
              tabIndex={0}
            >
              Minimize
            </button>
            <div role="separator" class="my-1 h-px bg-editor-divider" />
            <button
              type="button"
              role="menuitem"
              class="flex h-7 w-full items-center px-3 text-left outline-none hover:bg-red-500/85 hover:text-white focus-visible:bg-red-500/85 focus-visible:text-white"
              onClick={() => { runTauriWindowAction("close"); setSystemMenuOpen(false); }}
              tabIndex={0}
            >
              Close
            </button>
          </div>
        </Portal>
      </Show>
    </header>
  );
}
