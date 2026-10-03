import { createEffect } from "solid-js";
import { clsx } from "clsx";
import { LayerNode } from "@/engine/types";

interface LayerThumbProps {
  // Nullable: the properties panel's layer header can be torn down while the layer is
  // already gone, and this effect reads `props.layer` on the way out. A null layer
  // renders the bare checkerboard, which is what an empty slot looks like anyway.
  layer: LayerNode | null;
  isActive: boolean;
}

export function LayerThumb(props: LayerThumbProps) {
  let canvasRef: HTMLCanvasElement | undefined;

  createEffect(() => {
    const bitmap = props.layer?.imageBitmap;
    if (!canvasRef) return;

    const ctx = canvasRef.getContext("2d");
    if (!ctx) return;

    ctx.clearRect(0, 0, 34, 34);

    // Checkerboard
    for (let y = 0; y < 34; y += 6) {
      for (let x = 0; x < 34; x += 6) {
        ctx.fillStyle = (x + y) % 12 === 0 ? "rgba(255,255,255,0.05)" : "rgba(255,255,255,0.1)";
        ctx.fillRect(x, y, 6, 6);
      }
    }

    if (bitmap) {
      const w = bitmap.width;
      const h = bitmap.height;
      const scale = Math.min(34 / w, 34 / h);
      const nw = w * scale;
      const nh = h * scale;
      const dx = (34 - nw) / 2;
      const dy = (34 - nh) / 2;

      try {
        ctx.drawImage(bitmap, dx, dy, nw, nh);
      } catch {
        // bitmap may be closed/detached (snapshot reference)
      }
    }
  });

  return (
    <canvas
      ref={canvasRef}
      width={34}
      height={34}
      class={clsx(
        "size-[34px] shrink-0 rounded-[3px] border",
        props.isActive ? "border-editor-accent" : "border-black/40"
      )}
    />
  );
}
