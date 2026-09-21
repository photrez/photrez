/**
 * Wiring test for the facade-history-handoff re-upload sweep.
 *
 * After projecting a facade snapshot onto the engine, only layers the
 * projection added or swapped are re-uploaded, so the GPU texture cache
 * matches the engine (dropped-node reuse keeps pixels alive across routed
 * delete -> undo; without this the restored layer renders blank). Untouched
 * layers keep the same bitmap object, so their texture already matches and
 * the full re-upload is skipped. This mirrors the legacy restore sweep in
 * useEditorCommands.
 *
 * MOCK FIDELITY: facadeRegistry is mocked so getFacade / confirmExternalCursor
 * are deterministic; the engine stub lets the applyFacadeSnapshot mock mutate
 * the layer list the way the real projection does (in-place metadata for kept
 * layers, dropped-node re-add for restored ones). The renderer.uploadImage spy
 * proves which layers the sweep fires for. The two production flag
 * combinations are covered: the external-handoff branch (lastExternalHandoff
 * set, lastHistoryDeltaWasEmpty true) and the normal-delta branch (no
 * handoff, non-empty delta).
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { runFacadeExternalHandoff } from "../facadeHistoryHandoff";

vi.mock("@/lib/protocol/facadeRegistry", () => ({
  getFacade: vi.fn(),
  confirmExternalCursor: vi.fn(),
}));

import * as facadeRegistry from "@/lib/protocol/facadeRegistry";

const DOC_ID = "docHandoffReupload";
const bitmap = { width: 8, height: 8, close: vi.fn() } as unknown as ImageBitmap;

type StubLayer = { id: string; imageBitmap: ImageBitmap | null };

function makeEditor(layers: StubLayer[], onProject?: () => void) {
  const engine = {
    getId: () => DOC_ID,
    getLayers: () => layers,
    applyFacadeSnapshot: vi.fn(() => {
      onProject?.();
    }),
  };
  const renderer = { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() };
  const ctx = {
    workspace: {
      getActiveEngine: () => engine,
      notifyVisualChange: vi.fn(),
    },
    renderer,
    scheduler: { requestRender: vi.fn() },
  } as unknown as Parameters<typeof runFacadeExternalHandoff>[0];
  return { engine, renderer, ctx };
}

function makeFacade(flags: { external: boolean; emptyDelta: boolean }) {
  return {
    lastExternalHandoff: flags.external ? { seq: 1, direction: "undo" as const } : null,
    lastHistoryDeltaWasEmpty: flags.emptyDelta,
    undo: vi.fn(async () => ({ version: 1, layers: [] })),
    redo: vi.fn(async () => ({ version: 1, layers: [] })),
  };
}

describe("facade handoff re-upload sweep", () => {
  beforeEach(() => {
    vi.mocked(facadeRegistry.confirmExternalCursor).mockResolvedValue({ ok: true } as never);
  });

  it("external-handoff branch: uploads the restored layer, skips untouched and null layers", async () => {
    const facade = makeFacade({ external: true, emptyDelta: true });
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(facade as never);
    const restored = { width: 8, height: 8, close: vi.fn() } as unknown as ImageBitmap;
    const layers: StubLayer[] = [
      { id: "kept", imageBitmap: bitmap },
      { id: "nb", imageBitmap: null },
    ];
    const { ctx, renderer } = makeEditor(layers, () => {
      // Dropped-node reuse: the projection brings back a deleted layer with
      // its retained pixels; kept layers reuse their bitmap object.
      layers.push({ id: "rl", imageBitmap: restored });
    });
    await runFacadeExternalHandoff(ctx, "undo");

    expect(renderer.uploadImage).toHaveBeenCalledTimes(1);
    expect(renderer.uploadImage).toHaveBeenCalledWith("rl", restored);
    expect(renderer.uploadImage).not.toHaveBeenCalledWith("kept", bitmap);
  });

  it("normal-delta branch: uploads a swapped bitmap, skips untouched and null layers", async () => {
    const facade = makeFacade({ external: false, emptyDelta: false });
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(facade as never);
    const swapped = { width: 8, height: 8, close: vi.fn() } as unknown as ImageBitmap;
    const layers: StubLayer[] = [
      { id: "sw", imageBitmap: bitmap },
      { id: "kept", imageBitmap: bitmap },
      { id: "nb", imageBitmap: null },
    ];
    const { ctx, renderer } = makeEditor(layers, () => {
      layers[0] = { id: "sw", imageBitmap: swapped };
    });
    await runFacadeExternalHandoff(ctx, "undo");

    expect(renderer.uploadImage).toHaveBeenCalledTimes(1);
    expect(renderer.uploadImage).toHaveBeenCalledWith("sw", swapped);
    expect(renderer.uploadImage).not.toHaveBeenCalledWith("kept", bitmap);
    expect(renderer.uploadImage).not.toHaveBeenCalledWith("nb", null);
  });

  it("skips layers whose bitmap is unchanged by the projection", async () => {
    const facade = makeFacade({ external: false, emptyDelta: false });
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(facade as never);

    // The real applyFacadeSnapshot keeps the same bitmap object for
    // untouched layers (in-place metadata update, pixels stay host-side),
    // so the texture cache already matches and no upload is needed.
    const { ctx, renderer } = makeEditor([{ id: "same", imageBitmap: bitmap }]);
    await runFacadeExternalHandoff(ctx, "undo");

    expect(renderer.uploadImage).not.toHaveBeenCalledWith("same", bitmap);
  });

  it("changed layers re-upload FULL (no dirty rect): no covering region exists at this seam", async () => {
    // Fallback contract: the undo delta restates capture-time metadata (the
    // walker clones the entry capture, whose dirty_rect predates the undone
    // step), and the TS bitmaps are opaque handles, so no rect here provably
    // covers the changed pixels. Under-covering corrupts rendering, so the
    // sweep uploads FULL until a region producer exists at this seam.
    const facade = makeFacade({ external: false, emptyDelta: false });
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(facade as never);
    const swapped = { width: 8, height: 8, close: vi.fn() } as unknown as ImageBitmap;
    const layers: StubLayer[] = [{ id: "sw", imageBitmap: bitmap }];
    const { ctx, renderer } = makeEditor(layers, () => {
      layers[0] = { id: "sw", imageBitmap: swapped };
    });
    await runFacadeExternalHandoff(ctx, "undo");

    expect(renderer.uploadImage).toHaveBeenCalledTimes(1);
    expect(renderer.uploadImage).toHaveBeenCalledWith("sw", swapped);
    for (const call of renderer.uploadImage.mock.calls) expect(call.length).toBe(2);
  });
});
