/**
 * pixelLayerIds producer-threading wiring test.
 *
 * Contract: SnapshotEntry.pixelLayerIds makes the upload SKIP decision
 * explicit - null = unknown, [] = provably metadata-only, [ids] = only
 * those layers are upload candidates.
 *
 * After the history collapse the native walker owns undo, so the
 * upload-skip-on-undo consumer (the legacy TS pop reading the popped entry's
 * allowlist) is gone; only the producer threading survives:
 * commitLayerTransformSession threads [] because a transform session never
 * replaces a layer bitmap. A commit spy proves the allowlist reaches the entry.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { CommandHistory } from "@/engine/history";
import { commitLayerTransformSession } from "@/components/editor/transformSession";
import { invoke } from "@tauri-apps/api/core";
import * as DialogProviderModule from "../dialogs/DialogProvider";
import { releaseBitmapStore } from "@/engine/bitmapStore";
import type { DocumentModel } from "@/engine/types";

vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(() => Promise.resolve("0.0.0")),
}));

// The commit shim checks hasFacadeOwnedLayers() (Rust-owned history). Default off.
vi.mock("@/engine/document", () => ({
  hasFacadeOwnedLayers: vi.fn(() => false),
  facadeOwnedIdsOf: vi.fn(() => undefined),
  isFacadeOwnedLayer: vi.fn(() => false),
}));

function fakeBitmap(): ImageBitmap {
  return { width: 10, height: 10, close: vi.fn() } as unknown as ImageBitmap;
}

function makeModel(bitmap: ImageBitmap | null): DocumentModel {
  return {
    id: "doc-1",
    layers: [{ id: "l1", imageBitmap: bitmap }],
    activeLayerId: "l1",
  } as unknown as DocumentModel;
}

describe("pixelLayerIds producer threading", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({} as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValue({ version: 1, epoch: 0 });
    vi.mocked(isTauriRuntime).mockReset();
    vi.mocked(isTauriRuntime).mockReturnValue(false);
  });

  afterEach(() => {
    releaseBitmapStore("doc-1");
    vi.restoreAllMocks();
  });

  it("commitLayerTransformSession threads [] as the sole metadata-only producer", () => {
    const bm = fakeBitmap();
    const history = new CommandHistory();
    const before = makeModel(bm);
    const liveTransform = { x: 50, y: 30, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false };
    const engineStub = {
      getId: () => "doc-1",
      getLayer: () => ({ id: "l1", transform: liveTransform }),
    };

    const commitSpy = vi.spyOn(history, "commit");

    const ok = commitLayerTransformSession(
      {
        documentId: "doc-1",
        layerId: "l1",
        originalSnapshot: before,
        originalTransform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
        mode: "resize",
        lockRatio: false,
        startedAt: Date.now(),
      },
      engineStub as never,
      history,
    );

    expect(ok).toBe(true);
    expect(commitSpy).toHaveBeenCalledTimes(1);
    // 5th positional arg (pixelLayerIds) is the provably-metadata-only [].
    expect(commitSpy.mock.calls[0][4]).toEqual([]);
    // Commits the pre-session snapshot the entry must restore.
    expect(commitSpy.mock.calls[0][0]).toBe(before);
  });
});
