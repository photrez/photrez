import { DocumentEngine } from "@/engine/document";
import { saveProjectBinary } from "@/tauri/native";

export async function serializeAndSaveProject(engine: DocumentEngine, path: string): Promise<void> {
  const model = engine.snapshot();
  const layers: Record<string, Uint8Array> = {};

  for (const layer of model.layers) {
    if (layer.imageBitmap) {
      const canvas = new OffscreenCanvas(layer.width, layer.height);
      const ctx = canvas.getContext("2d");
      if (!ctx) {
        console.warn("projectSerialize: OffscreenCanvas.getContext('2d') returned null, skipping layer", layer.id);
        continue;
      }

      ctx.drawImage(layer.imageBitmap, 0, 0);
      const blob = await canvas.convertToBlob({ type: "image/png" });
      layers[layer.id] = new Uint8Array(await blob.arrayBuffer());
    }
  }

  const serializedModel = {
    ...model,
    format: "photrez-ptz",
    version: 1,
    layers: model.layers.map((l) => ({
      ...l,
      imageBitmap: null,
    })),
  };

  const documentJson = JSON.stringify(serializedModel);
  await saveProjectBinary(path, documentJson, layers);
}
