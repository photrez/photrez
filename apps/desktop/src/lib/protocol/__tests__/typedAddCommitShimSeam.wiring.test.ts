// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Production-seam wiring test for the facade typed-add snapshot refresh.
//
// The sibling typedAddFacadeSync.test.ts calls recordExternalTransitionFor
// DIRECTLY, which does not prove the refresh is reachable from the real app
// entry point. This test drives the PRODUCTION seam instead: it installs the
// commit shim (EditorShell does this once at boot), then calls
// CommandHistory.commit() - the exact call every legacy op makes. The shim
// forwards the LIVE engine into recordExternalTransitionFor, which re-projects
// the live layer vector into the facade snapshot.
//
// The typed layer is already in the snapshot before the commit (the
// post-mutation choke point, DocumentEngine.notifyChange, projected it), so the
// shim's re-projection is redundant by design here; this test proves the seam is
// reachable and that a commit with no following mutation leaves the layer in the
// snapshot instead of dropping it.
//
// Flag-ON path: every other shim test runs with photrez.facade OFF.

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { DEFAULT_TEXT_DATA } from "@/engine/textTypes";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import * as bridge from "@/lib/protocol/bridge";
import {
  getFacade,
  installFacadeCommitShim,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";

const DOC_ID = "docCommitSeam";

// jsdom has no OffscreenCanvas; stub the minimal seam the text rasterizer uses.
function stubOffscreenCanvas(): void {
  const Mock = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    const ctx: any = {
      font: "",
      fillStyle: "",
      strokeStyle: "",
      lineWidth: 0,
      lineJoin: "miter",
      miterLimit: 10,
      globalAlpha: 1,
      globalCompositeOperation: "source-over",
      textBaseline: "alphabetic",
      letterSpacing: undefined,
      measureText: (s: string) => ({
        width: s.length * 10,
        actualBoundingBoxAscent: 80,
        actualBoundingBoxDescent: 24,
        fontBoundingBoxAscent: 80,
        fontBoundingBoxDescent: 24,
      }),
      fillText: () => {},
      strokeText: () => {},
      drawImage: () => {},
      save: () => {},
      restore: () => {},
      translate: () => {},
      scale: () => {},
      rotate: () => {},
      fillRect: () => {},
    };
    this.getContext = () => ctx;
    this.transferToImageBitmap = () => ({ width: this.width, height: this.height, close: () => {} });
  } as unknown as typeof OffscreenCanvas;
  vi.stubGlobal("OffscreenCanvas", Mock);
}

// The shim reads getEngine()/getId() at commit time; hand it the per-test engine.
let liveEngine: DocumentEngine | null = null;

beforeAll(async () => {
  await getWasmExportModule();
  // Install the production shim once (shimInstalled is module-sticky; production
  // installs it once at EditorShell boot).
  installFacadeCommitShim({
    getEngine: () => liveEngine as never,
    getDocId: () => liveEngine?.getId() ?? "default",
  });
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
  localStorage.removeItem("photrez.facadeAuthority");
  stubOffscreenCanvas();
});

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  liveEngine = null;
  __resetFacadeRegistryForTests();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("facade commit-shim seam (photrez.facade=1)", () => {
  it("history.commit() through the installed shim keeps the typed layer in the facade snapshot", async () => {
    const engine = new DocumentEngine(DOC_ID, DOC_ID, 800, 600);
    liveEngine = engine;
    const facade = getFacade(DOC_ID);
    await seedFacadeFromEngine(engine as never, facade);
    const snap = await facade.addLayer("Owned", 100, 100, 0);
    engine.applyFacadeSnapshot(snap as never);

    const text = engine.addTextLayer("Hello", { ...DEFAULT_TEXT_DATA, content: "Hello" });
    // notifyChange is the post-mutation choke point, so the typed add is already
    // projected into the snapshot before any commit.
    expect(facade.snapshot.layers.map((l) => l.id)).toContain(text.id);

    // The production entry point every legacy op calls. No mutation follows, so
    // notifyChange will not run again. The layer must stay in the snapshot.
    const applyCommandSpy = vi.spyOn(bridge, "applyCommand");
    const history = new CommandHistory();
    history.commit(engine.getModel() as never, "Add Text");

    // Prove the shim actually reached the external-transition record; without
    // this the test would pass even if the shim path were dead code.
    await vi.waitFor(() => {
      expect(applyCommandSpy).toHaveBeenCalled();
      expect(facade.snapshot.layers.map((l) => l.id)).toContain(text.id);
    });
    // The routed layer that was already projected is still present too.
    expect(facade.snapshot.layers.map((l) => l.id)).toContain(snap.layers[0].id);
  });
});
