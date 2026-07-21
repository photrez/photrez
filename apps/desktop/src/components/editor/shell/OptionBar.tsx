import { lazy, Show, Suspense, type JSX } from "solid-js";
import { useEditor } from "./EditorContext";

function FadeIn(props: { children: JSX.Element }) {
  return <div class="animate-fade-in flex items-center gap-1.5">{props.children}</div>;
}

const MoveOptionBar = lazy(() => import("../MoveOptionBar").then(m => ({ default: m.MoveOptionBar })));
const CropOptionBar = lazy(() => import("../CropOptionBar").then(m => ({ default: m.CropOptionBar })));
const BrushOptionBar = lazy(() => import("../BrushOptionBar").then(m => ({ default: m.BrushOptionBar })));
const TransformOptionBar = lazy(() => import("../TransformOptionBar").then(m => ({ default: m.TransformOptionBar })));
const SelectionOptionBar = lazy(() => import("../SelectionOptionBar").then(m => ({ default: m.SelectionOptionBar })));
const EyedropperOptionBar = lazy(() => import("../EyedropperOptionBar").then(m => ({ default: m.EyedropperOptionBar })));
const PaintBucketOptionBar = lazy(() => import("../PaintBucketOptionBar").then(m => ({ default: m.PaintBucketOptionBar })));
const GradientOptionBar = lazy(() => import("../GradientOptionBar").then(m => ({ default: m.GradientOptionBar })));

export function OptionBar() {
  const { activeTool, layerTransformSession } = useEditor();

  return (
    <div class="@container flex h-[44px] shrink-0 items-center gap-1.5 border-b border-editor-divider bg-editor-toolbar px-3">
      <Suspense fallback={null}>
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
      </Suspense>
    </div>
  );
}
