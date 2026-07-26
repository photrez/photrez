import { Show, type JSX } from "solid-js";
import { useEditor } from "./EditorContext";
import { MoveOptionBar } from "../MoveOptionBar";
import { CropOptionBar } from "../CropOptionBar";
import { BrushOptionBar } from "../BrushOptionBar";
import { TransformOptionBar } from "../TransformOptionBar";
import { SelectionOptionBar } from "../SelectionOptionBar";
import { EyedropperOptionBar } from "../EyedropperOptionBar";
import { PaintBucketOptionBar } from "../PaintBucketOptionBar";
import { GradientOptionBar } from "../GradientOptionBar";

function FadeIn(props: { children: JSX.Element }) {
  return <div class="animate-fade-in flex items-center gap-1.5">{props.children}</div>;
}

export function OptionBar() {
  const { activeTool, layerTransformSession } = useEditor();

  return (
    <div class="@container flex h-[44px] shrink-0 items-center gap-1.5 border-b border-editor-divider bg-editor-toolbar px-3">
      <Show
        when={layerTransformSession()}
        fallback={
          <>
            <Show when={activeTool() === "selection"}>
              <FadeIn><SelectionOptionBar /></FadeIn>
            </Show>

            <Show when={activeTool() === "move"}>
              <FadeIn><MoveOptionBar /></FadeIn>
            </Show>

            <Show when={activeTool() === "crop"}>
              <FadeIn><CropOptionBar /></FadeIn>
            </Show>

            <Show when={activeTool() === "brush" || activeTool() === "eraser"}>
              <FadeIn><BrushOptionBar /></FadeIn>
            </Show>

            <Show when={activeTool() === "eyedropper"}>
              <FadeIn><EyedropperOptionBar /></FadeIn>
            </Show>

            <Show when={activeTool() === "paintBucket"}>
              <FadeIn><PaintBucketOptionBar /></FadeIn>
            </Show>

            <Show when={activeTool() === "gradient"}>
              <FadeIn><GradientOptionBar /></FadeIn>
            </Show>
          </>
        }
      >
        <FadeIn><TransformOptionBar /></FadeIn>
      </Show>
    </div>
  );
}
