/// <reference lib="webworker" />
// apps/desktop/src/components/editor/saveWorker.ts
//
// Web Worker that encodes a single layer's ImageBitmap to PNG bytes.
// Runs OffscreenCanvas inside the Worker so the main thread is not blocked.
//
// Message protocol:
//   → { type: "encode", layerId, width, height, imageBitmap }
//   ← { type: "result", layerId, pngBytes: Uint8Array }
//   ← { type: "error", layerId, error: string }

interface EncodeRequest {
  type: "encode";
  layerId: string;
  width: number;
  height: number;
  imageBitmap: ImageBitmap;
}

self.onmessage = async (e: MessageEvent<EncodeRequest>) => {
  const { type, layerId, width, height, imageBitmap } = e.data;

  if (type !== "encode") return;

  try {
    const canvas = new OffscreenCanvas(width, height);
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      self.postMessage({ type: "error", layerId, error: "OffscreenCanvas 2d context is null" });
      return;
    }

    ctx.drawImage(imageBitmap, 0, 0);
    imageBitmap.close(); // release GPU resources — worker lives across calls
    const blob = await canvas.convertToBlob({ type: "image/png" });
    const pngBytes = new Uint8Array(await blob.arrayBuffer());

    // Transfer the ArrayBuffer back for zero-copy delivery to the main thread.
    self.postMessage(
      { type: "result", layerId, pngBytes },
      { transfer: [pngBytes.buffer] },
    );
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    self.postMessage({ type: "error", layerId, error: message });
  }
};
