import { useEditor } from "./shell/EditorContext";

export function PaintBucketOptionBar() {
  const { fillTolerance, setFillTolerance } = useEditor();

  return (
    <div class="flex items-center gap-3 px-2 text-[11px]">
      <label class="flex items-center gap-1.5 text-editor-text-dim">
        Tolerance
        <input
          type="range"
          min={0}
          max={255}
          value={fillTolerance()}
          onInput={(e) => setFillTolerance(Number(e.currentTarget.value))}
          class="w-24 h-4 cursor-pointer"
        />
        <span class="w-6 text-right tabular-nums text-editor-text">
          {fillTolerance()}
        </span>
      </label>
    </div>
  );
}
