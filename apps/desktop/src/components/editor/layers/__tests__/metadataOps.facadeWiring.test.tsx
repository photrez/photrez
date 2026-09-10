// Call-site wiring for the four metadata ops (visibility / blendMode / lock /
// rename) routed through the native command arms behind the facade flag.
//
// Mirrors PropertiesPanel.facadeNumericGuard.test.tsx: facadeRegistry is
// MOCKED so the commit helpers are spies, isFacadeEnabled is a mutable flag,
// and isFacadeOwnedLayer is forced true (so any production call site that
// reaches the facade branch takes it). Each test fires the REAL production
// handler and asserts the commit helper is invoked (facade path) while the
// legacy engine method is NOT, and the inverse under the flag OFF.
//
// This proves the PRODUCTION dispatch (the part missing from the routed-op
// precedent), complementing facadeMetadataOps.test.ts which proves the helper
// envelope/projection/rejection at the bridge level.

import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider } from "../../shell/EditorContext";
import { LayersPanel } from "../LayersPanel";
import { WorkspaceManager } from "@/engine/workspace";
import * as Toast from "../../Toast";

const h = vi.hoisted(() => ({
  facadeOn: true,
  commitFacadeVisibility: vi.fn(() => Promise.resolve({ status: "applied", count: 1 })),
  commitFacadeRename: vi.fn(() => Promise.resolve({ status: "applied", count: 1 })),
  commitFacadeLock: vi.fn(() => Promise.resolve({ status: "applied", count: 1 })),
  commitFacadeBlendMode: vi.fn(() => Promise.resolve({ status: "applied", count: 1 })),
  commitFacadeReorder: vi.fn(() => Promise.resolve({ status: "applied", count: 1 })),
}));

vi.mock("@/lib/protocol/facadeRegistry", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isFacadeEnabled: () => h.facadeOn,
  commitFacadeVisibility: h.commitFacadeVisibility,
  commitFacadeRename: h.commitFacadeRename,
  commitFacadeLock: h.commitFacadeLock,
  commitFacadeBlendMode: h.commitFacadeBlendMode,
  commitFacadeReorder: h.commitFacadeReorder,
  MIXED_OWNERSHIP_MESSAGE: "mixed-selection",
}));

vi.mock("@/engine/document", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isFacadeOwnedLayer: () => true,
}));

function mount(session: ReturnType<typeof WorkspaceManager.createBlankDocument>) {
  const ws = new WorkspaceManager();
  ws.addDocument(session);
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as never} scheduler={scheduler as never}>
        <LayersPanel />
      </EditorProvider>
    ),
    container,
  );
  return { container, dispose, scheduler };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("metadata op call-site routing (facade dispatch)", () => {
  afterEach(() => {
    document.body.replaceChildren();
    vi.restoreAllMocks();
    h.facadeOn = true;
    h.commitFacadeVisibility.mockClear();
    h.commitFacadeRename.mockClear();
    h.commitFacadeLock.mockClear();
    h.commitFacadeBlendMode.mockClear();
    h.commitFacadeReorder.mockClear();
  });

  it("visibility toggle: flag ON routes to commitFacadeVisibility, legacy engine untouched", async () => {
    const session = WorkspaceManager.createBlankDocument("w-vis", "W", 800, 600);
    session.engine.addLayer("Top");
    const { container, dispose } = mount(session);
    await tick();
    const spy = vi.spyOn(session.engine, "setLayerVisibility");
    const btn = container.querySelector<HTMLButtonElement>("[data-layer-visibility]")!;
    btn.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await tick();
    expect(h.commitFacadeVisibility).toHaveBeenCalledTimes(1);
    expect(spy).not.toHaveBeenCalled();
    dispose();
  });

  it("visibility toggle: flag OFF uses legacy engine.setLayerVisibility, no facade commit", async () => {
    h.facadeOn = false;
    const session = WorkspaceManager.createBlankDocument("w-vis-off", "W", 800, 600);
    session.engine.addLayer("Top");
    const { container, dispose } = mount(session);
    await tick();
    const spy = vi.spyOn(session.engine, "setLayerVisibility");
    const btn = container.querySelector<HTMLButtonElement>("[data-layer-visibility]")!;
    btn.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await tick();
    expect(spy).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeVisibility).not.toHaveBeenCalled();
    dispose();
  });

  it("blend mode change: flag ON routes to commitFacadeBlendMode with the target id + mode", async () => {
    const session = WorkspaceManager.createBlankDocument("w-bld", "W", 800, 600);
    const top = session.engine.addLayer("Top");
    session.engine.setActiveLayer(top.id);
    const { container, dispose } = mount(session);
    await tick();
    const spy = vi.spyOn(session.engine, "setLayerBlendMode");
    const select = container.querySelector("select") as HTMLSelectElement;
    expect(select).toBeTruthy();
    const setter = Object.getOwnPropertyDescriptor(window.HTMLSelectElement.prototype, "value")!.set!;
    setter.call(select, "multiply");
    select.dispatchEvent(new Event("change", { bubbles: true }));
    await tick();
    expect(h.commitFacadeBlendMode).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeBlendMode.mock.calls[0]).toEqual(expect.arrayContaining([[top.id], "multiply"]));
    expect(spy).not.toHaveBeenCalled();
    dispose();
  });

  it("blend mode change: flag OFF uses legacy engine.setLayerBlendMode", async () => {
    h.facadeOn = false;
    const session = WorkspaceManager.createBlankDocument("w-bld-off", "W", 800, 600);
    const top = session.engine.addLayer("Top");
    session.engine.setActiveLayer(top.id);
    const { container, dispose } = mount(session);
    await tick();
    const spy = vi.spyOn(session.engine, "setLayerBlendMode");
    const select = container.querySelector("select") as HTMLSelectElement;
    const setter = Object.getOwnPropertyDescriptor(window.HTMLSelectElement.prototype, "value")!.set!;
    setter.call(select, "multiply");
    select.dispatchEvent(new Event("change", { bubbles: true }));
    await tick();
    expect(spy).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeBlendMode).not.toHaveBeenCalled();
    dispose();
  });

  it("blend mode change: flag OFF with all targets locked still pushes a history entry (legacy semantics preserved)", async () => {
    h.facadeOn = false;
    const session = WorkspaceManager.createBlankDocument("w-bld-locked", "W", 800, 600);
    const top = session.engine.addLayer("Top");
    session.engine.setLayerLocked(top.id, true);
    session.engine.setActiveLayer(top.id);
    const { container, dispose } = mount(session);
    await tick();
    const spy = vi.spyOn(session.engine, "setLayerBlendMode");
    const before = session.history.getUndoCount();
    const select = container.querySelector("select") as HTMLSelectElement;
    const setter = Object.getOwnPropertyDescriptor(window.HTMLSelectElement.prototype, "value")!.set!;
    setter.call(select, "multiply");
    select.dispatchEvent(new Event("change", { bubbles: true }));
    await tick();
    // The locked-target loop must skip the mutation, but the commit itself has
    // to happen first, exactly as the pre-facade handler did.
    expect(spy).not.toHaveBeenCalled();
    expect(session.history.getUndoCount()).toBeGreaterThan(before);
    expect(h.commitFacadeBlendMode).not.toHaveBeenCalled();
    dispose();
  });

  it("lock toggle: flag ON routes to commitFacadeLock (base kind) for the active layer", async () => {
    const session = WorkspaceManager.createBlankDocument("w-loc", "W", 800, 600);
    const top = session.engine.addLayer("Top");
    session.engine.setActiveLayer(top.id);
    const { container, dispose } = mount(session);
    await tick();
    const spy = vi.spyOn(session.engine, "setLayerLocked");
    const lockLabel = Array.from(container.querySelectorAll("span")).find((s) => s.textContent?.trim() === "Lock:");
    const lockBtn = lockLabel?.parentElement?.querySelector("button") as HTMLButtonElement | undefined;
    expect(lockBtn).toBeTruthy();
    lockBtn!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await tick();
    expect(h.commitFacadeLock).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeLock.mock.calls[0]).toEqual(expect.arrayContaining([[top.id], "base"]));
    expect(spy).not.toHaveBeenCalled();
    dispose();
  });

  it("rename: flag ON routes to commitFacadeRename via inline edit, legacy not touched", async () => {
    const session = WorkspaceManager.createBlankDocument("w-ren", "W", 800, 600);
    const top = session.engine.addLayer("Top");
    session.engine.setActiveLayer(top.id);
    const { container, dispose } = mount(session);
    await tick();
    const spy = vi.spyOn(session.engine, "setLayerName");
    // The layer panel renders top-of-stack first, so data-layer-idx is not a
    // stable handle for the Top layer. Target the Top row by its rendered name.
    const topRow = Array.from(container.querySelectorAll<HTMLElement>('[data-layer-idx]')).find((r) =>
      r.textContent?.includes("Top"),
    )!;
    topRow.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 40, clientY: 50 }));
    await tick();
    const renameItem = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')).find((b) =>
      b.textContent?.includes("Rename Layer"),
    );
    expect(renameItem).toBeTruthy();
    renameItem!.click();
    await tick();
    const input = container.querySelector("input[type='text']") as HTMLInputElement;
    expect(input).toBeTruthy();
    input.value = "Renamed";
    input.dispatchEvent(new InputEvent("input", { bubbles: true }));
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await tick();
    expect(h.commitFacadeRename).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeRename.mock.calls[0]).toEqual(expect.arrayContaining([[top.id], "Renamed"]));
    expect(spy).not.toHaveBeenCalled();
    dispose();
  });

  it("rejection path: a rejected commitFacadeVisibility surfaces as an error toast, no silent legacy mutation", async () => {
    const session = WorkspaceManager.createBlankDocument("w-rej", "W", 800, 600);
    session.engine.addLayer("Top");
    const toastSpy = vi.spyOn(Toast, "showToast");
    h.commitFacadeVisibility.mockImplementationOnce(() => Promise.reject(new Error("E_EXTERNAL_PENDING")));
    const { container, dispose } = mount(session);
    await tick();
    const spy = vi.spyOn(session.engine, "setLayerVisibility");
    const btn = container.querySelector<HTMLButtonElement>("[data-layer-visibility]")!;
    btn.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await tick();
    expect(toastSpy).toHaveBeenCalledWith(expect.stringContaining("E_EXTERNAL_PENDING"), "error");
    expect(spy).not.toHaveBeenCalled();
    dispose();
  });

  it("reorder move-down: flag ON routes to commitFacadeReorder with id + toIndex, legacy untouched", async () => {
    // Two non-background layers so Top can move down (Background is pinned to the bottom).
    const session = WorkspaceManager.createBlankDocument("w-reorder", "W", 800, 600);
    session.engine.addLayer("Mid");
    const top = session.engine.addLayer("Top");
    const { container, dispose } = mount(session);
    await tick();
    const spy = vi.spyOn(session.engine, "reorderLayer");
    const topRow = Array.from(container.querySelectorAll<HTMLElement>("[data-layer-idx]")).find((r) =>
      r.textContent?.includes("Top"),
    )!;
    const downBtn = topRow.querySelector<HTMLButtonElement>("[data-layer-move-down]")!;
    downBtn.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await tick();
    expect(h.commitFacadeReorder).toHaveBeenCalledTimes(1);
    expect(h.commitFacadeReorder).toHaveBeenCalledWith(expect.any(Object), top.id, 1);
    expect(spy).not.toHaveBeenCalled();
    dispose();
  });

  it("reorder move-down: flag OFF uses legacy engine.reorderLayer, no facade commit", async () => {
    h.facadeOn = false;
    // Two non-background layers so Top can move down (Background is pinned to the bottom).
    const session = WorkspaceManager.createBlankDocument("w-reorder-off", "W", 800, 600);
    session.engine.addLayer("Mid");
    const top = session.engine.addLayer("Top");
    const { container, dispose } = mount(session);
    await tick();
    const spy = vi.spyOn(session.engine, "reorderLayer");
    const topRow = Array.from(container.querySelectorAll<HTMLElement>("[data-layer-idx]")).find((r) =>
      r.textContent?.includes("Top"),
    )!;
    const downBtn = topRow.querySelector<HTMLButtonElement>("[data-layer-move-down]")!;
    downBtn.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await tick();
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy).toHaveBeenCalledWith(0, 1);
    expect(h.commitFacadeReorder).not.toHaveBeenCalled();
    dispose();
  });
});
