import { Show } from "solid-js";
import { useEditor } from "./shell/EditorContext";

export function LoadingOverlay() {
  const { loadingMessage } = useEditor();

  return (
    <Show when={loadingMessage() !== null}>
      <div class="fixed inset-x-0 top-[46px] bottom-0 z-50 flex items-center justify-center bg-black/40">
        <div class="flex flex-col items-center gap-4 rounded-lg bg-editor-panel border border-editor-divider px-8 py-6 shadow-xl">
          <svg style="animation: spin 1s linear infinite;" class="size-8 text-editor-accent" viewBox="0 0 24 24" fill="none">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
          </svg>
          <span class="text-[13px] text-editor-text">{loadingMessage()}</span>
        </div>
      </div>
    </Show>
  );
}
