/**
 * Wiring test for the facade-history-handoff re-upload sweep.
 *
 * After projecting a facade snapshot onto the engine, any layer that now carries
 * a retained bitmap must be re-uploaded to the renderer so the GPU texture cache
 * matches the engine (dropped-node reuse keeps pixels alive across routed
 * delete -> undo; without this the restored layer renders blank). This mirrors
 * the legacy restore sweep in useEditorCommands.
 *
 * MOCK FIDELITY: facadeRegistry is mocked so getFacade / confirmExternalCursor
 * are deterministic; the engine is a stub returning re-attached layers with
 * bitmaps, and the renderer.uploadImage spy proves the sweep fires for them.
 * The two production flag combinations are covered: the external-handoff branch
 * (lastExternalHandoff set, lastHistoryDeltaWasEmpty true) and the normal-delta
 * branch (no handoff, non-empty delta).
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

function makeEditor(layers: StubLayer[]) {
  const engine = {
    getId: () => DOC_ID,
    getLayers: () => layers,
    applyFacadeSnapshot: vi.fn(),
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

  it("external-handoff branch: re-uploads a retained-bitmap layer once", async () => {
    const facade = makeFacade({ external: true, emptyDelta: true });
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(facade as never);

    const { ctx, renderer } = makeEditor([{ id: "rl", imageBitmap: bitmap }]);
    await runFacadeExternalHandoff(ctx, "undo");

    expect(renderer.uploadImage).toHaveBeenCalledTimes(1);
    expect(renderer.uploadImage).toHaveBeenCalledWith("rl", bitmap);
  });

  it("normal-delta branch: uploads only layers that carry a bitmap (guards null)", async () => {
    const facade = makeFacade({ external: false, emptyDelta: false });
    vi.mocked(facadeRegistry.getFacade).mockReturnValue(facade as never);

    const { ctx, renderer } = makeEditor([
      { id: "rl", imageBitmap: bitmap },
      { id: "nb", imageBitmap: null },
    ]);
    await runFacadeExternalHandoff(ctx, "undo");

    expect(renderer.uploadImage).toHaveBeenCalledTimes(1);
    expect(renderer.uploadImage).toHaveBeenCalledWith("rl", bitmap);
    expect(renderer.uploadImage).not.toHaveBeenCalledWith("nb", null);
  });
});
