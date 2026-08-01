/// <reference lib="webworker" />
// apps/desktop/src/components/editor/saveWorker.ts
//
// Web Worker that encodes a single layer's ImageBitmap to PNG bytes.
// Runs OffscreenCanvas inside the Worker so the main thread is not blocked.
//
// Message protocol:
//   -> { type: "encode", layerId, width, height, imageBitmap }
//   -> { type: "reset" } — invalidates any in-flight encode (result is dropped)
//   <- { type: "result", layerId, pngBytes: Uint8Array }
//   <- { type: "error", layerId, error: string }

interface EncodeRequest {
  type: "encode";
  layerId: string;
  width: number;
  height: number;
  imageBitmap: ImageBitmap;
}

type WorkerRequest = EncodeRequest | { type: "reset" };

/**
 * Bumped on every reset message. An encode that completes after a reset
 * drops its result instead of posting it, so an aborted session can never
 * leak stale bytes into a later encodeLayers call. Keeps the worker alive
 * (no terminate/re-create churn) between an aborted autosave and the manual
 * save that follows.
 */
let resetToken = 0;

self.onmessage = async (e: MessageEvent<WorkerRequest>) => {
  if (e.data.type === "reset") {
    resetToken++;
    return;
  }

  const { layerId, width, height, imageBitmap } = e.data;
  const token = resetToken;

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
    if (token !== resetToken) return; // stale — a reset arrived mid-encode
    const pngBytes = new Uint8Array(await blob.arrayBuffer());

    // Transfer the ArrayBuffer back for zero-copy delivery to the main thread.
    self.postMessage(
      { type: "result", layerId, pngBytes },
      { transfer: [pngBytes.buffer] },
    );
  } catch (err) {
    if (token !== resetToken) return; // stale error from an aborted session
    const message = err instanceof Error ? err.message : String(err);
    self.postMessage({ type: "error", layerId, error: message });
  }
};
