import { Show, createMemo, type JSX } from "solid-js";
import { useEditor } from "./EditorContext";
import { MoveOptionBar } from "../MoveOptionBar";
import { CropOptionBar } from "../CropOptionBar";
import { BrushOptionBar } from "../BrushOptionBar";
import { TransformOptionBar } from "../TransformOptionBar";
import { SelectionOptionBar } from "../SelectionOptionBar";
import { EyedropperOptionBar } from "../EyedropperOptionBar";
import { PaintBucketOptionBar } from "../PaintBucketOptionBar";
import { GradientOptionBar } from "../GradientOptionBar";
import { ShapeOptionBar } from "../ShapeOptionBar";

function FadeIn(props: { children: JSX.Element }) {
  return <div class="animate-fade-in flex items-center gap-1.5">{props.children}</div>;
}

export function OptionBar() {
  const { activeTool, layerTransformSession, selectedLayerId, workspace } = useEditor();

  // Edit-mode mount: a parametric shape layer is selected (any active tool).
  // Guard against a null engine / raster layer so the bar never crashes.
  const isShapeLayerSelected = createMemo(() => {
    const id = selectedLayerId();
    if (!id) return false;
    const engine = workspace.getActiveEngine();
    if (!engine) return false;
    const layer = engine.getLayer(id);
    return !!layer && layer.type === "shape";
  });

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

            <Show when={activeTool() === "shape" || isShapeLayerSelected()}>
              <FadeIn><ShapeOptionBar /></FadeIn>
            </Show>
          </>
        }
      >
        <FadeIn><TransformOptionBar /></FadeIn>
      </Show>
    </div>
  );
}
