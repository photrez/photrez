// Phase E pilot — EditorClient.deleteLayer guard semantics.
//
// These tests drive EditorClient with a FAKE facade + engine + routing (no DOM,
// no localStorage) to prove the command/projection split and the ghost-layer
// guard independently of the production funnel:
//   - ghost-layer guard: a facade delete that no-ops (the victim id is left
//     PRESENT in the resulting snapshot) must surface as {status:"blocked"}
//     (NOT "facade") so the caller does NOT destroy the texture of a layer that
//     still exists. Fail-closed on the projection side: no engine apply runs.
//   - split-brain: a facade delete that SUCCEEDS (victim removed from the
//     snapshot) but whose projection THROWS must surface as {status:"facade",
//     error<defined>} (NOT "blocked") — the facade already mutated, so the
//     caller treats it as applied, not as a command failure.
// A third test proves applyFacadeSnapshot reclaims per-layer resource handles
// (paint surface + texture handle) for an id that vanished from the projection
// (no leak on a facade delete).

import { describe, it, expect, vi, beforeAll, afterAll, beforeEach, afterEach } from "vitest";
import { EditorClient } from "../editorClient";
import { EditorFacade } from "../editorFacade";
import type { RenderSnapshot } from "../types";
import { DocumentEngine } from "@/engine/document";
import {
  getFacade,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "../facadeRegistry";

// Minimal OffscreenCanvas stub so PaintTileSurface can exist under test
// (mirrors the stub in src/engine/__tests__/paintSurface.test.ts).
const OriginalOffscreenCanvas = (globalThis as any).OffscreenCanvas;
beforeAll(() => {
  if (typeof OffscreenCanvas === "undefined") {
    (globalThis as any).OffscreenCanvas = class {
      width: number;
      height: number;
      private data: Uint8ClampedArray;
      constructor(w: number, h: number) {
        this.width = w;
        this.height = h;
        this.data = new Uint8ClampedArray(w * h * 4);
      }
      getContext() {
        const self = this;
        return {
          drawImage(src: any) {
            const px = src?.__pixels as Uint8ClampedArray | undefined;
            if (!px || !src.width) return;
            const n = Math.min(self.width, src.width);
            const rows = Math.min(self.height, Math.floor(px.length / 4 / src.width));
            for (let y = 0; y < rows; y++)
              for (let x = 0; x < n; x++) {
                const s = (y * src.width + x) * 4,
                  d = (y * self.width + x) * 4;
                self.data[d] = px[s];
                self.data[d + 1] = px[s + 1];
                self.data[d + 2] = px[s + 2];
                self.data[d + 3] = px[s + 3];
              }
          },
          getImageData(sx: number, sy: number, sw: number, sh: number) {
            const out = new Uint8ClampedArray(sw * sh * 4);
            for (let y = 0; y < sh; y++)
              for (let x = 0; x < sw; x++) {
                const s = ((sy + y) * self.width + (sx + x)) * 4,
                  d = (y * sw + x) * 4;
                out[d] = self.data[s];
                out[d + 1] = self.data[s + 1];
                out[d + 2] = self.data[s + 2];
                out[d + 3] = self.data[s + 3];
              }
            return { data: out, width: sw, height: sh, colorSpace: "srgb" };
          },
          clearRect() {},
          save() {},
          restore() {},
          putImageData() {},
          globalCompositeOperation: "source-over",
        };
      }
    };
  }
});
afterAll(() => {
  if (OriginalOffscreenCanvas) (globalThis as any).OffscreenCanvas = OriginalOffscreenCanvas;
});

function fakeBitmap(w: number, h: number, fill: number): ImageBitmap {
  const px = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    px[i * 4] = fill;
    px[i * 4 + 1] = fill;
    px[i * 4 + 2] = fill;
    px[i * 4 + 3] = 255;
  }
  return { width: w, height: h, __pixels: px } as unknown as ImageBitmap;
}

const allOwned = () => ({ isFacadeEnabled: () => true, isFacadeOwnedLayer: () => true });

function snapOf(ids: string[]): RenderSnapshot {
  return {
    version: 1,
    layers: ids.map((id) => ({
      id,
      name: id,
      visible: true,
      opacity: 1,
      resourceId: 0,
      x: 0,
      y: 0,
      scaleX: 1,
      scaleY: 1,
      rotation: 0,
    })),
  };
}

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  __resetFacadeRegistryForTests();
  vi.restoreAllMocks();
});

describe("EditorClient.deleteLayer ghost-layer guard", () => {
  it("blocks a no-op facade delete (victim left present) as blocked, never facade", () => {
    const applied = vi.fn();
    const deleteLayer = vi
      .fn()
      // Emulates the emulation/Rust no-op for an id the facade does NOT know:
      // it returns a snapshot that STILL contains the victim (no Remove delta).
      .mockReturnValue(snapOf(["Keep", "Victim"]));
    const facade = { deleteLayer } as unknown as EditorFacade;
    const client = new EditorClient({ applyFacadeSnapshot: applied }, facade, allOwned());

    const res = client.deleteLayer("Victim");

    // The caller must treat this as blocked so it does NOT destroyTexture.
    expect(res.status).toBe("blocked");
    expect(res.status).not.toBe("facade");
    expect(res.error).toBe("E_NOOP_DELETE: facade left the layer present (unknown id)");
    // Fail-closed: the engine projection is never run, so no ghost is projected
    // out of the model (the renderer-visible layer set is untouched).
    expect(applied).not.toHaveBeenCalled();
    // No ghost: the facade view STILL contains the victim after the no-op.
    const returned = deleteLayer.mock.results[0].value as RenderSnapshot;
    expect(returned.layers.some((l) => l.id === "Victim")).toBe(true);
  });
});

describe("EditorClient.deleteLayer split-brain", () => {
  it("reports facade (not blocked) when the command succeeds but the projection throws", () => {
    const applied = vi.fn(() => {
      throw new Error("projection boom");
    });
    const deleteLayer = vi
      .fn()
      // The command SUCCEEDED: the victim is gone from the returned snapshot.
      .mockReturnValue(snapOf(["Keep"]));
    const facade = { deleteLayer } as unknown as EditorFacade;
    const client = new EditorClient({ applyFacadeSnapshot: applied }, facade, allOwned());

    const res = client.deleteLayer("Victim");

    // The facade already mutated: this is NOT a command failure, so status is
    // "facade" (caller must destroy the texture) with the projection error attached.
    expect(res.status).toBe("facade");
    expect(res.status).not.toBe("blocked");
    expect(res.snapshot).not.toBeNull();
    expect(res.error).toBe("projection boom");
    // The facade view reflects the successful delete (no victim).
    const returned = deleteLayer.mock.results[0].value as RenderSnapshot;
    expect(returned.layers.some((l) => l.id === "Victim")).toBe(false);
  });
});

describe("applyFacadeSnapshot leak-cleanup", () => {
  it("reclaims paint surface + texture handle for a layer id that vanished from the projection", () => {
    localStorage.setItem("photrez.facade", "1");
    const engine = new DocumentEngine("docLeak", "Leak", 64, 64);
    engine.addLayer("Background");
    const facade = getFacade("docLeak");
    seedFacadeFromEngine(engine as never, facade);

    // Add a victim THROUGH the facade and project it into the engine (the
    // production projection path). The engine now owns the victim metadata.
    const addSnap = facade.addLayer("Victim");
    engine.applyFacadeSnapshot(addSnap as never);
    const victim = addSnap.layers[addSnap.layers.length - 1];

    // Give the victim real per-layer resources: a cached paint surface + texture.
    engine.setLayerImageBitmap(victim.id, fakeBitmap(64, 64, 9));
    expect(engine.getPaintSurface(victim.id)).not.toBeNull();
    const tex = { id: "tex-victim" } as never;
    engine.setTextureHandle(victim.id, tex);
    expect(engine.getTextureHandle(victim.id)).toBeTruthy();

    // Delete via the facade (victim removed) and project the new snapshot.
    const delSnap = facade.deleteLayer(victim.id);
    expect(delSnap.layers.some((l) => l.id === victim.id)).toBe(false);
    engine.applyFacadeSnapshot(delSnap as never);

    // No leak: the vanished id no longer holds a paint surface or texture handle.
    expect(engine.getPaintSurface(victim.id)).toBeNull();
    expect(engine.getTextureHandle(victim.id)).toBeUndefined();
  });
});
