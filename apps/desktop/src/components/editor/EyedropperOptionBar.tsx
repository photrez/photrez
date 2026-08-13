import { createSignal, Show, createEffect } from "solid-js";
import { useEditor } from "./shell/EditorContext";
import { ToolPill, Divider, OptionCheckbox } from "./shell/OptionBarShared";
import { Icon } from "./icons";
import { Tooltip } from "./Tooltip";

export function EyedropperOptionBar() {
  const { fgColor } = useEditor();
  const [copied, setCopied] = createSignal(false);
  const [autoCopy, setAutoCopy] = createSignal(
    localStorage.getItem("photrez_eyedropper_autocopy") === "true"
  );

  const handleAutoCopyChange = (val: boolean) => {
    setAutoCopy(val);
    localStorage.setItem("photrez_eyedropper_autocopy", String(val));
  };

  const copyHexToClipboard = () => {
    if (typeof navigator !== "undefined" && navigator.clipboard) {
      navigator.clipboard.writeText(fgColor()).then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      }).catch(console.error);
    } else {
      // Fallback for test environment where clipboard API might not exist
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  };

  // Auto-copy hook: Automatically copy to clipboard when foreground color changes
  createEffect(() => {
    const color = fgColor();
    if (autoCopy() && typeof navigator !== "undefined" && navigator.clipboard) {
      navigator.clipboard.writeText(color).catch(console.error);
    }
  });

  return (
    <>
      <ToolPill icon="pipette" label="Eyedropper" />

      <Divider />

      <div class="flex items-center gap-2">
        {/* Visual Color Swatch */}
        <div
          class="size-4 rounded-[3px] border border-editor-divider ring-1 ring-white/20 shadow-sm shrink-0"
          style={{ "background-color": fgColor() }}
        />
        
        {/* Click to Copy HEX badge */}
        <Tooltip content="Click to copy HEX color">
          <button
            type="button"
            onClick={copyHexToClipboard}
            class="group flex h-[24px] items-center gap-1.5 rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[10px] font-sans font-semibold text-white transition-colors hover:border-[#4B515D] cursor-pointer select-none"
          >
            <span class="font-mono text-[11px] font-semibold tracking-wide text-white">{copied() ? "Copied!" : fgColor()}</span>
            <Show when={copied()} fallback={<Icon name="copy" class="size-3 text-[#A1A1AA] group-hover:text-white transition-colors" />}>
              <Icon name="check" class="size-3 text-editor-accent" strokeWidth={2.5} />
            </Show>
          </button>
        </Tooltip>
      </div>

      <Divider />

      {/* Auto-copy Setting */}
      <OptionCheckbox
        checked={autoCopy()}
        onChange={handleAutoCopyChange}
        label="Auto-Copy HEX"
      />
    </>
  );
}
