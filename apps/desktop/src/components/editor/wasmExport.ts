// SPDX-License-Identifier: AGPL-3.0-or-later

let wasmModulePromise: Promise<any> | null = null;

export async function getWasmExportModule(): Promise<any> {
  if (!wasmModulePromise) {
    wasmModulePromise = (async () => {
      try {
        // Dynamic import of the compiled WASM package
        const mod = await import("@/wasm/pkg/photrez_core");
        if (typeof mod.default === "function") {
          await mod.default();
        }
        return mod;
      } catch (err) {
        // Log warning for missing WASM pkg / non-WASM environment
        console.warn("WASM export module load failed, using Canvas fallback:", err);
        return null;
      }
    })();
  }
  return wasmModulePromise;
}

export async function encodeImageWithWasm(
  width: number,
  height: number,
  rgbaBytes: Uint8Array,
  formatStr: "png" | "jpeg" | "webp",
  quality: number,
): Promise<Uint8Array | null> {
  try {
    const wasmMod = await getWasmExportModule();
    if (!wasmMod || typeof wasmMod.encode_image_wasm !== "function") {
      return null;
    }
    const result = wasmMod.encode_image_wasm(width, height, rgbaBytes, formatStr, quality);
    return result ? new Uint8Array(result) : null;
  } catch (err) {
    console.warn("WASM image encoding failed, using Canvas fallback:", err);
    return null;
  }
}
