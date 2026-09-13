// Call-site wiring for basic-adjustment routing (SetAdjustment arm) behind the
// facade flag. Mirrors metadataOps.facadeWiring.test.tsx: facadeRegistry is
// MOCKED so commitFacadeAdjustment is a spy, isFacadeEnabled is a mutable flag,
// and each test fires the REAL AdjustmentsPanel slider/reset handler.
//
// Proves the production dispatch: flag ON routes apply/clear through
// commitFacadeAdjustment and skips the legacy engine mutation + TS history
// commit; flag OFF is byte-identical legacy; a mixed/rejected route surfaces a
// toast and mutates nothing.

import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider } from "../shell/EditorContext";
import { AdjustmentsPanel } from "../AdjustmentsPanel";
import { WorkspaceManager } from "@/engine/workspace";
import * as Toast from "../Toast";
import { adjustmentPreview, clearAdjustmentPreview } from "@/lib/protocol/facadeRegistry";

const h = vi.hoisted(() => ({
  facadeOn: true,
  commitFacadeAdjustment: vi.fn(() => Promise.resolve({ status: "applied", count: 1 })),
}));

vi.mock("@/lib/protocol/facadeRegistry", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isFacadeEnabled: () => h.facadeOn,
  commitFacadeAdjustment: h.commitFacadeAdjustment,
  MIXED_OWNERSHIP_MESSAGE: "mixed-selection",
}));

vi.mock("@/engine/document", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isFacadeOwnedLayer: () => true,
}));

function makeWorkspace(docId: string) {
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(docId, docId, 4, 2);
  ws.addDocument(session);
  const layer = session.engine.getLayers()[0];
  session.engine.setLayerImageBitmap(layer.id, { width: 4, height: 2 } as ImageBitmap);
  return { ws, session, layer };
}

function mount(ws: WorkspaceManager) {
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as never} scheduler={scheduler as never}>
        <AdjustmentsPanel />
      </EditorProvider>
    ),
    container,
  );
  return { container, dispose, scheduler };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

function fireBrightness(container: HTMLElement, value: string) {
  const slider = container.querySelector<HTMLInputElement>("input[aria-label='Bright']");
  if (!slider) throw new Error("Brightness slider not rendered");
  slider.value = value;
  slider.dispatchEvent(new InputEvent("input", { bubbles: true }));
}

// Slider gesture end: the range input's change event (also covered by
// pointerup/blur in the component). This is the boundary a routed drag commits at.
function fireCommit(container: HTMLElement) {
  const slider = container.querySelector<HTMLInputElement>("input[aria-label='Bright']");
  if (!slider) throw new Error("Brightness slider not rendered");
  slider.dispatchEvent(new Event("change", { bubbles: true }));
}

// A cancelled pointer (interrupted touch / system gesture steal) ends the drag
// too: the range input's pointercancel handler commits at the same boundary, so
// the transient preview and the pending gesture never stay stuck until the next
// blur or layer switch.
function firePointerCancel(container: HTMLElement) {
  const slider = container.querySelector<HTMLInputElement>("input[aria-label='Bright']");
  if (!slider) throw new Error("Brightness slider not rendered");
  slider.dispatchEvent(new Event("pointercancel", { bubbles: true }));
}

describe("basic-adjustment call-site routing (facade dispatch)", () => {
  afterEach(() => {
    document.body.replaceChildren();
    vi.restoreAllMocks();
    h.facadeOn = true;
    h.commitFacadeAdjustment.mockClear();
    clearAdjustmentPreview();
  });

  it("apply: flag ON routes to commitFacadeAdjustment; legacy apply + TS history untouched", async () => {
    const { ws, session, layer } = makeWorkspace("adj-on");
    const applySpy = vi.spyOn(session.engine, "applyBasicAdjustment");
    const commitSpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const { container, dispose } = mount(ws);
    await tick();

    fireBrightness(container, "25");
    // A tick is a drag in progress: no native dispatch until the gesture ends.
    expect(h.commitFacadeAdjustment).not.toHaveBeenCalled();
    fireCommit(container);
    await tick();

    expect(h.commitFacadeAdjustment).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeAdjustment).toHaveBeenCalledWith(
      expect.any(Object),
      [layer.id],
      { brightness: 25, contrast: 0, saturation: 0 },
    );
    expect(applySpy).not.toHaveBeenCalled();
    // Native arm commits its own history; no orphan TS entry may be created.
    expect(commitSpy).not.toHaveBeenCalled();
    dispose();
  });

  it("apply: flag OFF uses legacy engine.applyBasicAdjustment + one TS history commit, zero facade calls", async () => {
    h.facadeOn = false;
    const { ws, session, layer } = makeWorkspace("adj-off");
    const applySpy = vi.spyOn(session.engine, "applyBasicAdjustment");
    const commitSpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const { container, dispose } = mount(ws);
    await tick();

    fireBrightness(container, "25");
    await tick();

    expect(applySpy).toHaveBeenCalledWith(layer.id, { brightness: 25, contrast: 0, saturation: 0 });
    expect(commitSpy).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeAdjustment).not.toHaveBeenCalled();
    dispose();
  });

  it("apply: bitmap-less layer keeps the oracle no-op guard (no route, no legacy apply)", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("adj-nobmp", "N", 4, 2);
    ws.addDocument(session);
    const layer = session.engine.getLayers()[0];
    // Deliberately no bitmap: the legacy apply no-ops, so routing must too.
    const applySpy = vi.spyOn(session.engine, "applyBasicAdjustment");
    const { container, dispose } = mount(ws);
    await tick();

    fireBrightness(container, "25");
    await tick();

    expect(h.commitFacadeAdjustment).not.toHaveBeenCalled();
    expect(applySpy).not.toHaveBeenCalled();
    dispose();
  });

  it("reset: flag ON routes the clear (undefined) through commitFacadeAdjustment, legacy clear untouched", async () => {
    const { ws, session, layer } = makeWorkspace("adj-reset");
    // Seed a real adjustment so the panel's sync effect enables the reset button.
    session.engine.applyBasicAdjustment(layer.id, { brightness: 40, contrast: 0, saturation: 0 });
    const clearSpy = vi.spyOn(session.engine, "clearBasicAdjustments");
    const commitSpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const { container, dispose } = mount(ws);
    await tick();

    const reset = container.querySelector<HTMLButtonElement>("button[aria-label='Reset']");
    expect(reset).toBeTruthy();
    expect(reset!.disabled).toBe(false);
    reset!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await tick();

    expect(h.commitFacadeAdjustment).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeAdjustment).toHaveBeenCalledWith(expect.any(Object), [layer.id], undefined);
    expect(clearSpy).not.toHaveBeenCalled();
    expect(commitSpy).not.toHaveBeenCalled();
    dispose();
  });

  it("mixed-rejected: surfaces the ownership toast and mutates nothing", async () => {
    h.commitFacadeAdjustment.mockImplementationOnce(() => Promise.resolve({ status: "mixed-rejected", count: 0 }));
    const toastSpy = vi.spyOn(Toast, "showToast");
    const { ws, session } = makeWorkspace("adj-mixed");
    const applySpy = vi.spyOn(session.engine, "applyBasicAdjustment");
    const commitSpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const { container, dispose } = mount(ws);
    await tick();

    fireBrightness(container, "25");
    fireCommit(container);
    await tick();

    expect(toastSpy).toHaveBeenCalledWith("mixed-selection", "error");
    expect(applySpy).not.toHaveBeenCalled();
    expect(commitSpy).not.toHaveBeenCalled();
    // The optimistic panel value is resynced to the unchanged model.
    expect(container.querySelector<HTMLInputElement>("input[aria-label='Bright']")!.value).toBe("0");
    dispose();
  });

  it("throw: surfaces an error toast and does not fall through to the legacy mutation", async () => {
    h.commitFacadeAdjustment.mockImplementationOnce(() => Promise.reject(new Error("E_EXTERNAL_PENDING")));
    const toastSpy = vi.spyOn(Toast, "showToast");
    const { ws, session } = makeWorkspace("adj-throw");
    const applySpy = vi.spyOn(session.engine, "applyBasicAdjustment");
    const { container, dispose } = mount(ws);
    await tick();

    fireBrightness(container, "25");
    fireCommit(container);
    await tick();

    expect(toastSpy).toHaveBeenCalledWith(expect.stringContaining("E_EXTERNAL_PENDING"), "error");
    expect(applySpy).not.toHaveBeenCalled();
    // The optimistic panel value is resynced to the unchanged model.
    expect(container.querySelector<HTMLInputElement>("input[aria-label='Bright']")!.value).toBe("0");
    dispose();
  });

  it("rejected earlier route does not clobber a newer in-flight gesture", async () => {
    // Gesture 1's dispatch stays pending; gesture 2 starts dragging before it
    // settles. When gesture 1 rejects, its failure handler must NOT resync the
    // slider from the model — that would make gesture 2 read moved=false at its
    // boundary and silently drop its commit.
    let rejectFirst: (e: unknown) => void = () => {};
    h.commitFacadeAdjustment
      .mockImplementationOnce(
        () => new Promise<{ status: string; count: number }>((_resolve, reject) => { rejectFirst = reject; }),
      )
      .mockImplementationOnce(() => Promise.resolve({ status: "applied", count: 1 }));

    const { ws } = makeWorkspace("adj-overlap");
    const { container, dispose } = mount(ws);
    await tick();

    // Gesture 1: drag then gesture end -> dispatch queued (promise pending).
    fireBrightness(container, "10");
    fireCommit(container);
    await tick();
    expect(h.commitFacadeAdjustment).toHaveBeenCalledTimes(1);

    // Gesture 2 starts while gesture 1 is still in flight.
    fireBrightness(container, "40");
    expect(adjustmentPreview()?.adjustment).toEqual({ brightness: 40, contrast: 0, saturation: 0 });

    // Gesture 1 rejects while gesture 2 owns the preview.
    rejectFirst(new Error("E_VERSION_MISMATCH"));
    await tick();

    // The newer gesture keeps its value and live preview.
    const slider = container.querySelector<HTMLInputElement>("input[aria-label='Bright']")!;
    expect(slider.value).toBe("40");
    expect(adjustmentPreview()?.adjustment).toEqual({ brightness: 40, contrast: 0, saturation: 0 });

    // Gesture 2's boundary still commits its final value.
    fireCommit(container);
    await tick();
    expect(h.commitFacadeAdjustment).toHaveBeenCalledTimes(2);
    expect(h.commitFacadeAdjustment).toHaveBeenLastCalledWith(
      expect.any(Object),
      [expect.any(String)],
      { brightness: 40, contrast: 0, saturation: 0 },
    );
    dispose();
  });

  it("drag: three moves then gesture end => exactly ONE native dispatch, preview live each tick", async () => {
    const { ws, session, layer } = makeWorkspace("adj-drag");
    const applySpy = vi.spyOn(session.engine, "applyBasicAdjustment");
    const commitSpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const { container, dispose } = mount(ws);
    await tick();

    fireBrightness(container, "10");
    expect(adjustmentPreview()?.adjustment).toEqual({ brightness: 10, contrast: 0, saturation: 0 });
    fireBrightness(container, "20");
    expect(adjustmentPreview()?.adjustment).toEqual({ brightness: 20, contrast: 0, saturation: 0 });
    fireBrightness(container, "30");
    expect(adjustmentPreview()?.adjustment).toEqual({ brightness: 30, contrast: 0, saturation: 0 });

    // Every tick stayed transient: zero native dispatch, zero model/history mutation.
    expect(h.commitFacadeAdjustment).not.toHaveBeenCalled();
    expect(applySpy).not.toHaveBeenCalled();
    expect(commitSpy).not.toHaveBeenCalled();

    fireCommit(container);
    await tick();

    // ONE gesture-boundary commit carrying the final value; preview cleared.
    expect(adjustmentPreview()).toBeNull();
    expect(h.commitFacadeAdjustment).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeAdjustment).toHaveBeenCalledWith(
      expect.any(Object),
      [layer.id],
      { brightness: 30, contrast: 0, saturation: 0 },
    );
    expect(applySpy).not.toHaveBeenCalled();
    expect(commitSpy).not.toHaveBeenCalled();
    dispose();
  });

  it("interrupted drag: pointercancel commits at the gesture boundary and clears the preview", async () => {
    const { ws, session, layer } = makeWorkspace("adj-cancel");
    const applySpy = vi.spyOn(session.engine, "applyBasicAdjustment");
    const commitSpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const { container, dispose } = mount(ws);
    await tick();

    fireBrightness(container, "42");
    expect(adjustmentPreview()?.adjustment).toEqual({ brightness: 42, contrast: 0, saturation: 0 });
    expect(h.commitFacadeAdjustment).not.toHaveBeenCalled();

    firePointerCancel(container);
    await tick();

    // The interrupted gesture committed exactly once and left no pending preview.
    expect(adjustmentPreview()).toBeNull();
    expect(h.commitFacadeAdjustment).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeAdjustment).toHaveBeenCalledWith(
      expect.any(Object),
      [layer.id],
      { brightness: 42, contrast: 0, saturation: 0 },
    );
    expect(applySpy).not.toHaveBeenCalled();
    expect(commitSpy).not.toHaveBeenCalled();
    dispose();
  });

  it("flag ON + funnel defers to legacy: one TS history commit + legacy model apply, no native arm", async () => {
    h.commitFacadeAdjustment.mockImplementationOnce(() =>
      Promise.resolve({ status: "legacy", count: 0 }),
    );
    const { ws, session, layer } = makeWorkspace("adj-legacy-fallback");
    const applySpy = vi.spyOn(session.engine, "applyBasicAdjustment");
    const commitSpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const { container, dispose } = mount(ws);
    await tick();

    fireBrightness(container, "10");
    fireBrightness(container, "25");
    fireCommit(container);
    await tick();

    // The funnel was consulted once and deferred, so the untouched legacy path
    // ran: one coalesced TS entry + one final model apply, no native command.
    expect(h.commitFacadeAdjustment).toHaveBeenCalledTimes(1);
    expect(applySpy).toHaveBeenCalledWith(layer.id, { brightness: 25, contrast: 0, saturation: 0 });
    expect(commitSpy).toHaveBeenCalledTimes(1);
    dispose();
  });

  it("locked layer: flag ON routes nothing and commits no TS entry (legacy oracle)", async () => {
    const { ws, session, layer } = makeWorkspace("adj-locked");
    layer.locked = true;
    const applySpy = vi.spyOn(session.engine, "applyBasicAdjustment");
    const commitSpy = vi.spyOn(ws.getActiveHistory()!, "commit");
    const { container, dispose } = mount(ws);
    await tick();

    fireBrightness(container, "25");
    fireCommit(container);
    await tick();

    // commitAdjustmentSession skips locked layers, so the routed path must too.
    expect(h.commitFacadeAdjustment).not.toHaveBeenCalled();
    expect(commitSpy).not.toHaveBeenCalled();
    // The render param still updates (legacy applies it; only the undo entry is skipped).
    expect(applySpy).toHaveBeenCalledWith(layer.id, { brightness: 25, contrast: 0, saturation: 0 });
    dispose();
  });
});
