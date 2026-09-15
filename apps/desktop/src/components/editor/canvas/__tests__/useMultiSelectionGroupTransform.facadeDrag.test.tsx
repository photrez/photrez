// Group transform (multi-selection bounding-box handle) while the members are
// owned by the native editor state.
//
// Contracts pinned here, none of which a pure-function test can see:
//  1. ZERO protocol calls during the gesture. pointermove fires 50+ fps, so one
//     command per frame per member is the failure this path exists to prevent. The
//     only dispatches are the commits at pointerup, asserted at the single boundary
//     every protocol command crosses (bridge applyCommand), not at a facade stub.
//  2. Every member is previewed, so no member renders frozen.
//  3. The facade's one transient transform slot comes down on EVERY exit that does
//     not commit. The numeric commit funnel refuses while the slot is held, so a
//     leaked slot turns every later numeric edit of that document into a permanent
//     refusal.
//  4. Both resize modes of the gesture: Shift (every axis tracks the pointer) and
//     no Shift (aspect ratio held, the default for most users). The routed commit
//     carries absolute per-member values, so it has to land where the legacy
//     per-frame mutator landed. The parity cases in the last block are what pin that.
//
// Real hook, real DocumentEngine, real history, real protocol arm.
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { createRoot, createSignal } from "solid-js";
import { useMultiSelectionGroupTransform } from "../useMultiSelectionGroupTransform";
import { DocumentEngine, isFacadeOwnedLayer } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import type { LayerNode, Transform2D } from "@/engine/types";
import {
  CORNER,
  FREE_AXIS_CORNER,
  PROPORTIONAL_TARGET,
  RESIZE_GESTURES,
  TARGET,
  expectedMemberTransforms,
  withPositionLock,
  type Gesture,
  type MemberStart,
} from "./groupResizeReference";
import {
  MIXED_OWNERSHIP_MESSAGE,
  __resetFacadeRegistryForTests,
  facadeCommitNumericTransform,
  getFacade,
  peekFacade,
  removeFacade,
  seedFacadeFromEngine,
  transformPreview,
} from "@/lib/protocol/facadeRegistry";
import { getWasmExportModule } from "../../wasmExport";
import { showToast } from "../../Toast";
import type { CommandEnvelope } from "@/lib/protocol/types";

vi.mock("../../Toast", () => ({ showToast: vi.fn() }));
const { mockEditorState } = vi.hoisted(() => ({ mockEditorState: {} as Record<string, unknown> }));
vi.mock("../../shell/EditorContext", () => ({ useEditor: () => mockEditorState }));

// applyCommand is the one function every protocol command crosses, under the wasm
// arm and the native arm alike, so its call list IS the dispatch count.
const { commandLog } = vi.hoisted(() => ({ commandLog: { types: [] as string[] } }));
vi.mock("@/lib/protocol/bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/protocol/bridge")>();
  return {
    ...actual,
    applyCommand: (envelope: CommandEnvelope) => {
      commandLog.types.push(envelope.command.type);
      return actual.applyCommand(envelope);
    },
  };
});

const DOC = "group-transform-facade";
// A case that runs one fixture per authority needs two documents: the facade registry
// and the native document are both keyed by id, and the owned-layer set is global.
const LEGACY_DOC = "group-transform-legacy-authority";
const ROUTED_DOC = "group-transform-routed-authority";

let wasm: { protocol_reset: (docId: string) => void } | null = null;
const usedDocs: string[] = [];

beforeAll(async () => {
  wasm = await getWasmExportModule();
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
});

afterEach(() => {
  for (const id of usedDocs) wasm?.protocol_reset(id);
  usedDocs.length = 0;
  localStorage.removeItem("photrez.facade");
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  vi.restoreAllMocks();
});

/** Every way a group gesture can end without applying itself. */
type Exit =
  | "pointercancel"
  | "escape"
  | "lostpointercapture"
  | "members-deleted"
  | "document-switched"
  | "no-active-engine"
  | "pointerup-without-commit"
  | "unmount";

interface Ctx {
  engine: DocumentEngine;
  history: CommandHistory;
  owned: string[];
  legacyId: string;
  /** Which authority this fixture's members belong to: true = native editor state. */
  routed: boolean;
  setSelected: (ids: string[]) => void;
  /** The gesture API, reached through the container listeners below. */
  api: () => ReturnType<typeof useMultiSelectionGroupTransform>;
  dispose: () => void;
  commands: () => string[];
  /** What the viewport's HUD readout was last fed: null means the gesture closed it. */
  lastHud: () => unknown;
  reset: () => void;
  down: (handle?: string, x?: number, y?: number) => void;
  /** `shiftKey` defaults to true: every case written before the proportional branch
   *  got its own coverage holds Shift, so each axis tracks the pointer exactly. */
  move: (x: number, y: number, shiftKey?: boolean) => void;
  up: (x?: number, y?: number) => void;
  cancel: () => void;
  escape: () => void;
  lostCapture: () => void;
  setActiveEngine: (e: DocumentEngine | null) => void;
  deleteOwned: (id: string) => Promise<void>;
  otherEngine: DocumentEngine;
}

async function harness(opts: {
  owned: number;
  legacy?: boolean;
  flag?: boolean;
  lockPositionOf?: number;
  lockRotationOf?: number;
  /** Override when one case needs two fixtures side by side: the facade registry and
   *  the native document are both keyed by id, so two live fixtures need two ids. */
  doc?: string;
}): Promise<Ctx> {
  const doc = opts.doc ?? DOC;
  usedDocs.push(doc);
  const flagOn = opts.flag !== false;
  // Set it, not just clear the other way: a case that builds one fixture per authority
  // shares this module-level switch with the fixture it built before it.
  if (flagOn) localStorage.setItem("photrez.facade", "1");
  else localStorage.removeItem("photrez.facade");

  const engine = new DocumentEngine(doc, "Group", 800, 600);
  const facade = getFacade(doc);
  if (flagOn) await seedFacadeFromEngine(engine as never, facade);

  const owned: string[] = [];
  const spots = [
    { x: 100, y: 100 },
    { x: 400, y: 100 },
    { x: 100, y: 400 },
  ];
  for (let i = 0; i < opts.owned; i++) {
    const spot = spots[i % spots.length];
    if (flagOn) {
      // The snapshot projection carries no bitmap dims, so a facade-created layer
      // lands in the model at document size. Give the fixture its real size once,
      // here, so the group anchor math runs against the same box a user would see.
      engine.applyFacadeSnapshot(await facade.addLayer(`Owned ${i}`, 200, 200));
      const node = engine.getLayers().find((l) => l.name === `Owned ${i}`);
      if (!node) throw new Error("setup: facade-created layer did not project into the engine");
      node.width = 200;
      node.height = 200;
      await facadeCommitNumericTransform(engine, node.id, { ...spot });
      owned.push(node.id);
    } else {
      const node = engine.addLayer(`Owned ${i}`, 200, 200);
      // Park through the engine mutator, not by writing the node: the Rust graph
      // mirror restates the model on the next add, and a TS-only write would be
      // wiped by that restatement.
      engine.transformLayer(node.id, { x: spot.x, y: spot.y });
      owned.push(node.id);
    }
  }
  // Locks go on after every member exists. The graph mirror restates the model on each
  // add, so a lock set mid-loop is not guaranteed to survive the next layer.
  for (const [i, id] of owned.entries()) {
    for (const kind of ["position", "rotation"] as const) {
      const wanted = kind === "position" ? opts.lockPositionOf === i : opts.lockRotationOf === i;
      if (!wanted) continue;
      if (flagOn) {
        engine.applyFacadeSnapshot(await facade.setLayerLocked(id, kind, true));
      } else if (kind === "position") {
        engine.setLayerLockPosition(id, true);
      } else {
        engine.setLayerLockRotation(id, true);
      }
    }
  }
  const legacyId = opts.legacy ? engine.addLayer("Legacy", 200, 200).id : "";

  const history = new CommandHistory();
  const scheduler = { requestRender: vi.fn() };
  const otherEngine = new DocumentEngine(`${doc}-other`, "Other", 800, 600);
  let activeEngine: DocumentEngine | null = engine;
  const workspace = {
    getActiveEngine: () => activeEngine,
    getActiveHistory: () => history,
    notifyVisualChange: vi.fn(),
  };

  const [selectedLayerIds, setSelectedLayerIds] = createSignal<string[]>([]);
  const onHudUpdate = vi.fn<(hud: unknown) => void>();

  let api!: ReturnType<typeof useMultiSelectionGroupTransform>;
  let dispose!: () => void;
  createRoot((d) => {
    dispose = d;
    Object.assign(mockEditorState, {
      workspace,
      scheduler,
      layers: () => engine.getLayers() as unknown as LayerNode[],
      selectedLayerIds,
      zoom: () => 1,
      pan: () => ({ x: 0, y: 0 }),
      activeTool: () => "move",
    });
    api = useMultiSelectionGroupTransform({
      onHudUpdate,
      // Identity mapping: the gesture math is in document units and the fixture
      // parks layers in document units.
      onScreenToDoc: (cx, cy) => ({ x: cx, y: cy }),
    });
  });

  // The container the viewport mounts these handlers on. Pointer capture is not
  // modelled by jsdom, so it is stubbed out rather than left to throw.
  const container = document.createElement("div");
  container.setPointerCapture = () => {};
  container.releasePointerCapture = () => {};
  document.body.appendChild(container);
  let handle = "se";
  container.addEventListener("pointerdown", (e) => api.handlePointerDown(e as PointerEvent, handle));
  container.addEventListener("pointermove", (e) => api.handlePointerMove(e as PointerEvent));
  container.addEventListener("pointerup", (e) => api.handlePointerUp(e as PointerEvent));
  container.addEventListener("pointercancel", (e) => api.handlePointerCancel(e as PointerEvent));

  // Shift breaks the aspect lock, so each axis tracks the pointer exactly. Without it
  // the drag is proportional: one factor for both axes, derived from the handle.
  const fire = (type: string, x: number, y: number, shiftKey = false) =>
    container.dispatchEvent(
      new PointerEvent(type, {
        bubbles: true,
        cancelable: true,
        clientX: x,
        clientY: y,
        pointerId: 1,
        shiftKey,
      }),
    );

  const ctx: Ctx = {
    engine,
    history,
    owned,
    legacyId,
    routed: flagOn,
    setSelected: (ids) => setSelectedLayerIds(ids),
    api: () => api,
    dispose,
    commands: () => [...commandLog.types],
    lastHud: () => onHudUpdate.mock.calls.at(-1)?.[0],
    reset: () => {
      commandLog.types.length = 0;
    },
    down: (h = "se", x = CORNER.x, y = CORNER.y) => {
      handle = h;
      fire("pointerdown", x, y);
    },
    move: (x, y, shiftKey = true) => fire("pointermove", x, y, shiftKey),
    up: (x = CORNER.x, y = CORNER.y) => fire("pointerup", x, y),
    cancel: () => fire("pointercancel", CORNER.x, CORNER.y),
    escape: () => window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })),
    lostCapture: () => fire("lostpointercapture", CORNER.x, CORNER.y),
    setActiveEngine: (e) => {
      activeEngine = e;
    },
    deleteOwned: async (id) => {
      engine.applyFacadeSnapshot(await facade.deleteLayer(id));
      ctx.reset();
    },
    otherEngine,
  };
  ctx.reset();
  return ctx;
}

const transformOf = (ctx: Ctx, id: string): Transform2D => ({ ...ctx.engine.getLayer(id)!.transform });
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

/**
 * Select the first two members and take the SE handle, ending with `target` previewed
 * and nothing dispatched. `shift` picks the resize mode: held, every axis tracks the
 * pointer; released, the box keeps its aspect ratio and one factor drives both.
 */
async function beginGesture(
  ctx: Ctx,
  handle = "se",
  frames = 3,
  target = TARGET,
  shift = true,
) {
  ctx.setSelected([ctx.owned[0], ctx.owned[1]]);
  ctx.down(handle);
  expect(ctx.api().isTransforming()).toBe(true);
  expect(peekFacade(DOC)!.transientTransformActive()).toBe(true);
  ctx.reset();
  for (let i = 1; i < frames; i++) {
    ctx.move(CORNER.x + i * 20, CORNER.y + i * 10, shift);
    expect(ctx.commands()).toEqual([]);
  }
  ctx.move(target.x, target.y, shift);
  expect(ctx.commands()).toEqual([]);
}

/**
 * One complete handle gesture, from grip to commit, on either authority.
 *
 * The intermediate frames exist to catch a per-frame protocol call on the routed path:
 * every frame is recomputed absolutely from where the gesture started, so only the last
 * one can end up committed, on either authority.
 */
async function runGesture(ctx: Ctx, gesture: Gesture) {
  const [a, b] = ctx.owned;
  ctx.setSelected([a, b]);
  const group = ctx.api().groupAabb();
  const starts: MemberStart[] = [a, b].map((id) => {
    const node = ctx.engine.getLayer(id);
    if (!node) throw new Error(`setup: member ${id} is not in the model`);
    // Normalized, not copied: a TS-created node leaves the lock flags undefined where a
    // snapshot-projected one writes them false, and both read as unlocked.
    return {
      transform: { ...node.transform },
      lockPosition: !!node.lockPosition,
      lockRotation: !!node.lockRotation,
    };
  });
  const undoBefore = ctx.history.getUndoCount();

  if (!group) throw new Error("setup: the fixture selection does not produce a group box");
  ctx.down(gesture.handle, gesture.from.x, gesture.from.y);
  expect(ctx.api().activeHandle()).toBe(gesture.handle);
  ctx.reset();

  for (const step of [1, 2, 3]) {
    ctx.move(
      gesture.from.x + ((gesture.to.x - gesture.from.x) * step) / 3,
      gesture.from.y + ((gesture.to.y - gesture.from.y) * step) / 3,
      gesture.shift,
    );
    expect(ctx.commands(), `${gesture.label}: frame ${step} dispatched a command`).toEqual([]);
  }
  // Read after the last frame and before the release: this is what the user sees while
  // the pointer is down, and the routed path is the only one that has a separate answer
  // for it.
  const previewed = transformPreview().map((p) => ({ ...p.transform }));

  ctx.up(gesture.to.x, gesture.to.y);
  if (ctx.routed) {
    await vi.waitFor(() => expect(ctx.commands()).toEqual(["transformLayer", "transformLayer"]));
  }
  const frames = expectedMemberTransforms(
    gesture,
    group,
    starts.map((start) => start.transform),
  );
  return {
    commands: ctx.commands(),
    /** Where the resize would put every member with no lock in the way. */
    frames,
    /** The same, after the model mutator's lock rule. */
    expected: starts.map((start, i) => withPositionLock(start, frames[i])),
    group,
    members: [transformOf(ctx, a), transformOf(ctx, b)],
    previewed,
    starts,
    undoSteps: ctx.history.getUndoCount() - undoBefore,
  };
}

describe("group transform on native-owned layers", () => {
  it("preview reaches the render channel for every member, with zero protocol calls", async () => {
    const ctx = await harness({ owned: 2 });
    try {
      const [a, b] = ctx.owned;
      const beforeA = transformOf(ctx, a);
      const beforeB = transformOf(ctx, b);
      ctx.setSelected([a, b]);
      ctx.down();
      ctx.reset();

      ctx.move(TARGET.x, TARGET.y);
      // The model is untouched mid-gesture, so it cannot be what the user sees.
      expect(transformOf(ctx, a)).toEqual(beforeA);
      expect(transformOf(ctx, b)).toEqual(beforeB);
      // This is the signal applyFacadePreviews merges into the outgoing RenderState
      // (EditorShell's render scheduler), i.e. the pixels during the drag. Both
      // members are in it: a member without a preview renders frozen.
      expect(transformPreview()).toEqual([
        { layerId: a, transform: { ...beforeA, scaleX: 2, scaleY: 2 } },
        { layerId: b, transform: { ...beforeB, x: 700, scaleX: 2, scaleY: 2 } },
      ]);
      // And the group overlay reads the same channel, so the box and its handles
      // track the preview instead of sitting still.
      expect(ctx.api().groupAabb()).toEqual({ x: 100, y: 100, width: 1000, height: 400 });
      expect(ctx.commands()).toEqual([]);
    } finally {
      ctx.dispose();
    }
  });

  it("one gesture dispatches exactly one command per member and writes no history entry", async () => {
    const ctx = await harness({ owned: 2 });
    try {
      const [a, b] = ctx.owned;
      const beforeA = transformOf(ctx, a);
      const beforeB = transformOf(ctx, b);
      const undoBefore = ctx.history.getUndoCount();
      await beginGesture(ctx, "se", 12);

      ctx.up(TARGET.x, TARGET.y);
      await vi.waitFor(() => expect(ctx.commands()).toEqual(["transformLayer", "transformLayer"]));
      expect(transformOf(ctx, a)).toEqual({ ...beforeA, scaleX: 2, scaleY: 2 });
      expect(transformOf(ctx, b)).toEqual({ ...beforeB, x: 700, scaleX: 2, scaleY: 2 });
      expect(transformPreview()).toEqual([]);
      expect(peekFacade(DOC)!.transientTransformActive()).toBe(false);
      // The native transform arm owns the undo entry: a second one here would leave
      // two undo steps for one gesture.
      expect(ctx.history.getUndoCount()).toBe(undoBefore);
    } finally {
      ctx.dispose();
    }
  });

  it("rotate handle: one command per member, rotation and pivot land", async () => {
    const ctx = await harness({ owned: 2 });
    try {
      const [a, b] = ctx.owned;
      const beforeA = transformOf(ctx, a);
      const beforeB = transformOf(ctx, b);
      ctx.setSelected([a, b]);
      // Anchor the gesture on the top edge midpoint of the group box, offset above
      // it: the rotate ring and the top pin both use handle "rotate".
      ctx.down("rotate", 350, 60);
      ctx.reset();

      // Swing the pointer to the left of the group centre: a 15-degree snap step.
      ctx.move(100, 150);
      const preview = transformPreview();
      expect(preview.map((p) => p.layerId)).toEqual([a, b]);
      expect(ctx.commands()).toEqual([]);
      const rotatedA = preview[0]!.transform;
      const rotatedB = preview[1]!.transform;
      expect(rotatedA.rotation).not.toBe(beforeA.rotation);

      ctx.up(100, 150);
      await vi.waitFor(() => expect(ctx.commands()).toEqual(["transformLayer", "transformLayer"]));
      expect(transformOf(ctx, a)).toEqual(rotatedA);
      expect(transformOf(ctx, b)).toEqual(rotatedB);
      expect(transformPreview()).toEqual([]);
    } finally {
      ctx.dispose();
    }
  });

  /**
   * The position lock, pinned on the member this gesture actually displaces, on both
   * authorities, against remembered numbers.
   *
   * Member 1 starts at (400,100), 300 px right of the group box's own corner, so the drag
   * below carries it to x 700: that travel is what the lock has to prevent. Locking member
   * 0 instead would prove nothing, because it sits on the NW corner the SE grip holds still,
   * so its x/y stay at 100 with or without the lock.
   *
   * Routed pre-filters the mask onto the preview and onto the commit patch (memberTransform
   * in useMultiSelectionGroupTransform.ts). Legacy filters it inside the model write: the
   * Rust mirror while engine/document.ts transformLayer can hand the frame to it, and
   * engine/layerOps.ts transformLayer on the fallback. Both were measured, not assumed:
   * dropping the two lockPosition branches of memberTransform fails the "routed" assertion
   * below with member 1 at x 700, and removing the guard in layerOps.ts fails the "legacy"
   * one once document.ts is made to take the fallback path.
   */
  it("a position-locked member scales in place and is never moved", async () => {
    const legacy = await harness({ owned: 2, lockPositionOf: 1, flag: false, doc: LEGACY_DOC });
    const routed = await harness({ owned: 2, lockPositionOf: 1, doc: ROUTED_DOC });
    try {
      for (const ctx of [legacy, routed]) {
        const label = ctx.routed ? "routed" : "legacy";
        const [a, b] = ctx.owned;
        const beforeA = transformOf(ctx, a);
        const beforeB = transformOf(ctx, b);
        ctx.setSelected([a, b]);
        const group = ctx.api().groupAabb();
        if (!group) throw new Error("setup: the fixture selection does not produce a group box");
        // Pre-condition, from the shared geometry and not from the code under test: with no
        // lock in the way this drag puts member 1 at x 700. Without it, "the lock held" and
        // "the gesture moved nothing" would be the same observation.
        const unlocked = expectedMemberTransforms(FREE_AXIS_CORNER, group, [beforeA, beforeB]);
        expect(unlocked[1], `${label}: pre-condition`).toEqual({ ...beforeB, x: 700, scaleX: 2, scaleY: 2 });

        ctx.down();
        ctx.reset();
        ctx.move(TARGET.x, TARGET.y);

        // What the user sees while the pointer is down: the preview on the routed authority
        // (its model stays frozen), the model itself on the legacy one (no preview
        // channel). The locked member reads the same on either.
        const seen = ctx.routed
          ? transformPreview().find((p) => p.layerId === b)?.transform
          : transformOf(ctx, b);
        expect(seen, `${label}: locked member mid-gesture`).toEqual({ ...beforeB, scaleX: 2, scaleY: 2 });
        expect(transformPreview().map((p) => p.layerId), `${label}: preview channel`).toEqual(ctx.routed ? [a, b] : []);
        expect(ctx.commands(), `${label}: mid-gesture dispatch`).toEqual([]);

        ctx.up(TARGET.x, TARGET.y);
        if (ctx.routed) {
          await vi.waitFor(() => expect(ctx.commands()).toEqual(["transformLayer", "transformLayer"]));
        } else {
          expect(ctx.commands(), `${label}: legacy commit went out as a protocol command`).toEqual([]);
        }
        // The commit lands where the gesture showed it: x/y kept, scale applied. The
        // unlocked member of the same gesture lands on the un-locked reference, so the
        // gesture demonstrably ran while the locked one stayed put.
        expect(transformOf(ctx, b), `${label}: locked member after commit`).toEqual({ ...beforeB, scaleX: 2, scaleY: 2 });
        expect(transformOf(ctx, a), `${label}: unlocked member after commit`).toEqual(unlocked[0]);
      }
    } finally {
      legacy.dispose();
      routed.dispose();
    }
  });

  it("flag OFF: the same chain still mutates the model per frame and writes one history entry", async () => {
    const ctx = await harness({ owned: 2, flag: false });
    try {
      const [a, b] = ctx.owned;
      const beforeA = transformOf(ctx, a);
      const undoBefore = ctx.history.getUndoCount();
      ctx.setSelected([a, b]);
      ctx.down();
      ctx.reset();

      ctx.move(700, 350);
      // Synchronous, no await: the legacy gesture must not gain a microtask hop.
      expect(transformOf(ctx, a)).toEqual({ ...beforeA, scaleX: 1.2, scaleY: 1.25 });
      expect(transformPreview()).toEqual([]);
      expect(ctx.commands()).toEqual([]);
      ctx.move(TARGET.x, TARGET.y);

      ctx.up(TARGET.x, TARGET.y);
      expect(ctx.commands()).toEqual([]);
      expect(transformOf(ctx, a)).toEqual({ ...beforeA, scaleX: 2, scaleY: 2 });
      expect(transformOf(ctx, b)).toEqual({ x: 700, y: 100, scaleX: 2, scaleY: 2, rotation: 0, flipH: false, flipV: false });
      expect(ctx.history.getUndoCount()).toBe(undoBefore + 1);
      expect(ctx.history.getHistoryStack().some((h) => h.label === "Transform Layers")).toBe(true);
    } finally {
      ctx.dispose();
    }
  });

  it("mixed ownership: the gesture is refused with a toast, nothing moves, no slot is taken", async () => {
    const ctx = await harness({ owned: 1, legacy: true });
    try {
      const [a] = ctx.owned;
      const before = transformOf(ctx, a);
      const legacyBefore = transformOf(ctx, ctx.legacyId);
      ctx.setSelected([a, ctx.legacyId]);

      ctx.down();
      ctx.move(TARGET.x, TARGET.y);
      ctx.up(TARGET.x, TARGET.y);
      await settle();

      expect(ctx.api().isTransforming()).toBe(false);
      expect(showToast).toHaveBeenCalledWith(MIXED_OWNERSHIP_MESSAGE, "error");
      expect(ctx.commands()).toEqual([]);
      expect(transformOf(ctx, a)).toEqual(before);
      expect(transformOf(ctx, ctx.legacyId)).toEqual(legacyBefore);
      expect(peekFacade(DOC)!.transientTransformActive()).toBe(false);
      expect(transformPreview()).toEqual([]);
    } finally {
      ctx.dispose();
    }
  });

  it("one member deleted mid-gesture: no crash, no leak, the survivor still lands", async () => {
    const ctx = await harness({ owned: 3 });
    try {
      const [a, b, c] = ctx.owned;
      const beforeC = transformOf(ctx, c);
      ctx.setSelected([a, b]);
      // Member c sits at (100,400): outside the selection, but 300 px below the corner
      // this drag doubles away from, so it would land at y 700 if the gesture ever reached
      // past its own members. That is what makes "c is untouched" below an observation
      // instead of a tautology about a layer nothing could move anyway.
      const groupBefore = ctx.api().groupAabb();
      if (!groupBefore) throw new Error("setup: the fixture selection does not produce a group box");
      expect(expectedMemberTransforms(FREE_AXIS_CORNER, groupBefore, [beforeC])[0].y).toBe(700);
      ctx.down();
      ctx.reset();
      ctx.move(CORNER.x + 100, CORNER.y + 50);
      expect(transformPreview().map((p) => p.layerId)).toEqual([a, b]);

      await ctx.deleteOwned(a);
      ctx.move(TARGET.x, TARGET.y);
      // The deleted member is out of the gesture; the survivor is still previewed.
      expect(transformPreview().map((p) => p.layerId)).toEqual([b]);
      expect(ctx.api().isTransforming()).toBe(true);
      expect(ctx.commands()).toEqual([]);

      ctx.up(TARGET.x, TARGET.y);
      await vi.waitFor(() => expect(ctx.commands()).toEqual(["transformLayer"]));
      expect(transformOf(ctx, b).x).toBe(700);
      expect(transformOf(ctx, c)).toEqual(beforeC);
      expect(peekFacade(DOC)!.transientTransformActive()).toBe(false);
      expect(transformPreview()).toEqual([]);
    } finally {
      ctx.dispose();
    }
  });

  it("one member deselected mid-gesture: it is left where it was, the survivor still lands", async () => {
    const ctx = await harness({ owned: 3 });
    try {
      const [a, b, c] = ctx.owned;
      const beforeA = transformOf(ctx, a);
      await beginGesture(ctx);

      ctx.setSelected([b]);
      ctx.move(TARGET.x, TARGET.y);
      expect(transformPreview().map((p) => p.layerId)).toEqual([b]);
      ctx.up(TARGET.x, TARGET.y);
      await vi.waitFor(() => expect(ctx.commands()).toEqual(["transformLayer"]));
      expect(transformOf(ctx, a)).toEqual(beforeA);
      expect(transformOf(ctx, b).x).toBe(700);
      expect(transformOf(ctx, c).x).toBe(100);
      // x alone would not tell, (100,400) sits in the anchor column of this box. The y and
      // the scale are the parts a gesture that reached past its selection would have taken
      // to 700 and 2.
      expect([transformOf(ctx, c).y, transformOf(ctx, c).scaleX]).toEqual([400, 1]);
      expect(peekFacade(DOC)!.transientTransformActive()).toBe(false);
    } finally {
      ctx.dispose();
    }
  });

  it("a numeric edit issued during the gesture is refused, and the gesture still commits", async () => {
    const ctx = await harness({ owned: 3 });
    try {
      const dragged = ctx.owned[0];
      const spare = ctx.owned[2];
      await beginGesture(ctx);

      await expect(facadeCommitNumericTransform(ctx.engine, spare, { x: 5 })).rejects.toThrow(
        /transform gesture is in progress/,
      );
      expect(ctx.engine.getLayer(spare)!.transform.x).not.toBe(5);

      ctx.up(TARGET.x, TARGET.y);
      await vi.waitFor(() => expect(ctx.commands()).toEqual(["transformLayer", "transformLayer"]));
      expect(ctx.engine.getLayer(dragged)).toBeTruthy();
      // The gesture landed even though the refusal above happened inside it, so the
      // refusal cost it nothing.
      expect(ctx.engine.getLayer(ctx.owned[1])!.transform.x).toBe(700);
      // And the same numeric edit that was refused mid-gesture goes through now that the
      // slot is down: that makes the rejects above a gate on the gesture rather than a
      // commit seam that could never have written x anyway.
      await expect(facadeCommitNumericTransform(ctx.engine, spare, { x: 5 })).resolves.toBe(true);
      expect(ctx.engine.getLayer(spare)!.transform.x).toBe(5);
    } finally {
      ctx.dispose();
    }
  });

  it("releasing after the document was closed does not resurrect the facade it evicted", async () => {
    const ctx = await harness({ owned: 2 });
    try {
      await beginGesture(ctx);
      removeFacade(DOC);
      ctx.cancel();
      expect(peekFacade(DOC)).toBeUndefined();
      expect(transformPreview()).toEqual([]);
    } finally {
      ctx.dispose();
    }
  });

  it("two consecutive gestures leave no orphan preview and no held slot", async () => {
    const ctx = await harness({ owned: 2 });
    try {
      await beginGesture(ctx);
      ctx.up(TARGET.x, TARGET.y);
      await vi.waitFor(() => expect(ctx.commands()).toEqual(["transformLayer", "transformLayer"]));
      expect(transformPreview()).toEqual([]);
      expect(peekFacade(DOC)!.transientTransformActive()).toBe(false);

      const afterFirst = transformOf(ctx, ctx.owned[1]);
      ctx.down();
      ctx.reset();
      ctx.move(CORNER.x + 60, CORNER.y + 40);
      expect(ctx.commands()).toEqual([]);
      ctx.up(CORNER.x + 60, CORNER.y + 40);
      await vi.waitFor(() => expect(ctx.commands()).toEqual(["transformLayer", "transformLayer"]));
      expect(transformOf(ctx, ctx.owned[1]).x).toBeGreaterThan(afterFirst.x);
      expect(transformPreview()).toEqual([]);
      expect(peekFacade(DOC)!.transientTransformActive()).toBe(false);
    } finally {
      ctx.dispose();
    }
  });

  it("a plain click on the handle dispatches nothing and writes no history entry", async () => {
    const ctx = await harness({ owned: 2 });
    try {
      const [a, b] = ctx.owned;
      const beforeA = transformOf(ctx, a);
      const undoBefore = ctx.history.getUndoCount();
      ctx.setSelected([a, b]);
      ctx.down();
      ctx.reset();
      ctx.up();
      await settle();

      expect(ctx.commands()).toEqual([]);
      expect(transformOf(ctx, a)).toEqual(beforeA);
      expect(ctx.history.getUndoCount()).toBe(undoBefore);
      expect(peekFacade(DOC)!.transientTransformActive()).toBe(false);
    } finally {
      ctx.dispose();
    }
  });
});

describe("abandoned group gestures release the transient commit slot", () => {
  const exits: Exit[] = [
    "pointercancel",
    "escape",
    "lostpointercapture",
    "members-deleted",
    "document-switched",
    "no-active-engine",
    "pointerup-without-commit",
    "unmount",
  ];

  /**
   * Both resize modes, so every exit is proven to release the slot for the
   * proportional branch too and not only for the axis-free one the rest of the file
   * uses. The two modes land on different values, so a leaked preview would differ.
   */
  const modes = [
    { mode: "free-axis", shift: true, to: TARGET },
    { mode: "proportional", shift: false, to: PROPORTIONAL_TARGET },
  ];

  it.each(exits.flatMap((exit) => modes.map(({ mode, shift, to }) => ({ exit, mode, shift, to }))))(
    "$mode, $exit: no command, preview cleared, slot free, a later numeric commit still lands",
    async ({ exit, shift, to }) => {
      const ctx = await harness({ owned: 3 });
      try {
        const [a, b] = ctx.owned;
        const beforeA = transformOf(ctx, a);
        const beforeB = transformOf(ctx, b);
        await beginGesture(ctx, "se", 3, to, shift);
        // The gesture holds the slot: this is what the release has to give back.
        await expect(facadeCommitNumericTransform(ctx.engine, ctx.owned[2], { x: 7 })).rejects.toThrow(
          /transform gesture is in progress/,
        );
        ctx.reset();

        switch (exit) {
          case "pointercancel":
            ctx.cancel();
            break;
          case "escape":
            ctx.escape();
            break;
          case "lostpointercapture":
            ctx.lostCapture();
            break;
          case "members-deleted":
            await ctx.deleteOwned(a);
            await ctx.deleteOwned(b);
            ctx.move(to.x, to.y, shift);
            break;
          case "document-switched":
            // A tab switch under the gesture: the members do not exist in the newly
            // active document, so nothing can commit them.
            ctx.setActiveEngine(ctx.otherEngine);
            ctx.move(to.x, to.y, shift);
            ctx.setActiveEngine(ctx.engine);
            break;
          case "no-active-engine":
            ctx.setActiveEngine(null);
            ctx.move(to.x, to.y, shift);
            ctx.setActiveEngine(ctx.engine);
            break;
          case "pointerup-without-commit":
            // The commit branch is skipped, so only the release at the top of the
            // handler can give the slot back.
            ctx.setActiveEngine(null);
            ctx.up(to.x, to.y);
            ctx.setActiveEngine(ctx.engine);
            break;
          case "unmount":
            ctx.dispose();
            break;
        }

        await settle();
        expect(ctx.commands(), `${exit} dispatched a command instead of dropping the gesture}`).toEqual([]);
        if (exit !== "unmount") {
          expect(ctx.api().isTransforming(), `${exit} left the gesture live`).toBe(false);
          if (exit !== "members-deleted") {
            // The abandoned gesture applied nothing: its members stayed where they
            // were. When the members themselves are gone there is no transform left
            // to compare, and the numeric commit below proves the slot came down.
            expect(transformOf(ctx, a), `${exit} applied the abandoned gesture`).toEqual(beforeA);
            expect(transformOf(ctx, b), `${exit} applied the abandoned gesture`).toEqual(beforeB);
          }
        }
        expect(peekFacade(DOC)?.transientTransformActive(), `${exit} leaked the facade slot`).toBeFalsy();
        expect(transformPreview(), `${exit} left a preview behind`).toEqual([]);
        // The regression a leaked slot causes: this refusal would be permanent, with
        // no handle visible to release. The gesture's own members may be gone, so
        // edit an untouched one.
        await expect(facadeCommitNumericTransform(ctx.engine, ctx.owned[2], { x: 55 })).resolves.toBe(true);
        expect(ctx.engine.getLayer(ctx.owned[2])!.transform.x).toBe(55);
      } finally {
        ctx.dispose();
      }
    },
  );

  it("escape: preview cleared and the HUD closed, with the model at its pre-gesture value", async () => {
    const ctx = await harness({ owned: 2 });
    try {
      const [a, b] = ctx.owned;
      const beforeA = transformOf(ctx, a);
      await beginGesture(ctx);
      expect(transformPreview().length).toBe(2);
      // The readout is open with live numbers before the abort, so the null below can only
      // come from the cancel path closing it.
      expect(ctx.lastHud()).toMatchObject({ mode: "resize", width: 1000 });

      ctx.escape();

      expect(ctx.api().isTransforming()).toBe(false);
      expect(ctx.lastHud(), "escape left the readout open").toBeNull();
      expect(ctx.commands()).toEqual([]);
      expect(transformOf(ctx, a)).toEqual(beforeA);
      expect(transformOf(ctx, b).x).toBe(400);
      expect(transformPreview()).toEqual([]);
      expect(ctx.api().groupAabb()).toEqual({ x: 100, y: 100, width: 500, height: 200 });
    } finally {
      ctx.dispose();
    }
  });
});

describe("proportional (no Shift) group resize", () => {
  it.each(RESIZE_GESTURES)(
    "$label: legacy and routed leave every member on the same transform",
    async (gesture) => {
      const legacy = await harness({ owned: 2, flag: false, doc: LEGACY_DOC });
      const routed = await harness({ owned: 2, doc: ROUTED_DOC });
      try {
        const onLegacy = await runGesture(legacy, gesture);
        const onRouted = await runGesture(routed, gesture);

        // The comparison is only worth anything from the same place. Both fixtures park
        // A at (100,100) and B at (400,100), one through the engine mutator and one
        // through the native commit funnel, so this checks the fixtures rather than the
        // gesture.
        expect([onRouted.group, onRouted.starts]).toEqual([onLegacy.group, onLegacy.starts]);
        // And the gesture has to have moved something, or two authorities that both did
        // nothing would agree perfectly.
        expect(onRouted.frames[1].x).not.toBe(onRouted.starts[1].transform.x);
        // The remembered numbers, checked against both computed answers.
        expect(onLegacy.members[1]).toMatchObject(gesture.pin);
        expect(onRouted.members[1]).toMatchObject(gesture.pin);

        // Each authority agrees with the reference geometry...
        expect(onLegacy.members).toEqual(onLegacy.expected);
        expect(onRouted.members).toEqual(onRouted.expected);
        // PARITY: ...and therefore with each other. This is the assertion the case exists
        // for: the commit sends the previewed values instead of re-deriving them at
        // release, so a member that previewed nothing, or a routed patch that dropped a
        // field the per-frame mutator used to carry, fails here.
        expect(onRouted.members).toEqual(onLegacy.members);
        // Only the bookkeeping differs, and it differs by design: the native transform
        // arm owns the routed gesture's undo entry.
        expect(onLegacy.undoSteps).toBe(1);
        expect(onRouted.undoSteps).toBe(0);
      } finally {
        legacy.dispose();
        routed.dispose();
      }
    },
  );

  it("proportional resize dispatches one command per member, none during the moves, and no TS history entry", async () => {
    const ctx = await harness({ owned: 2 });
    const gesture = RESIZE_GESTURES[0];
    try {
      const run = await runGesture(ctx, gesture);

      // The whole travel went out at release, in member order, once each.
      expect(run.commands).toEqual(["transformLayer", "transformLayer"]);
      expect(run.undoSteps).toBe(0);
      // What the user saw while the pointer was down is what got committed: preview and
      // model cannot disagree about a member.
      expect(run.previewed).toEqual(run.expected);
      expect(run.members).toEqual(run.expected);
      expect(run.members[1]).toMatchObject(gesture.pin);
      // The grip was dragged 290 right and 0 down, and the height still grew by the same
      // factor as the width. That is the aspect lock, so these values can only come from
      // the proportional branch: the free-axis branch would have left scaleY at 1.
      expect(gesture.to.y - gesture.from.y).toBe(0);
      expect(run.members[1].scaleX).toBe(run.members[1].scaleY);
      expect(peekFacade(DOC)!.transientTransformActive()).toBe(false);
      expect(transformPreview()).toEqual([]);
    } finally {
      ctx.dispose();
    }
  });

  /**
   * The locked-member question, answered by running both resize modes against both
   * authorities rather than by reading one branch of it. The legacy per-frame mutator
   * applies the locks inside the model (layerOps.transformLayer skips x/y for a
   * position-locked layer and still writes its scale), while the routed path pre-applies
   * that same mask to the preview and to the commit patch. They MATCH, including the
   * disclosed quirk that a position-locked member still receives scale. A rotation lock
   * has nothing to defend in a resize at all: neither mode writes a rotation for either
   * authority.
   */
  it.each([RESIZE_GESTURES[0], FREE_AXIS_CORNER])(
    "$label: a position-locked member keeps x/y, still takes the scale, on either authority",
    async (gesture) => {
      // Member 1 is the one the resize would carry from (400,100) outward, so its lock is
      // the one that can actually be observed. Member 0 sits on the anchor and does not
      // move with or without a lock, which is why locking it would prove nothing.
      const lockFixture = { owned: 2, lockPositionOf: 1, lockRotationOf: 0 };
      const legacy = await harness({ ...lockFixture, flag: false, doc: LEGACY_DOC });
      const routed = await harness({ ...lockFixture, doc: ROUTED_DOC });
      try {
        const onLegacy = await runGesture(legacy, gesture);
        const onRouted = await runGesture(routed, gesture);

        // The locks really are on, on both fixtures: an unlocked fixture would let every
        // assertion below pass for the wrong reason.
        expect(onLegacy.starts).toEqual(onRouted.starts);
        expect(onRouted.starts.map((s) => [s.lockPosition, s.lockRotation])).toEqual([
          [false, true],
          [true, false],
        ]);
        // And the gesture would have moved that member had the lock not been there.
        expect(onRouted.frames[1].x).not.toBe(onRouted.starts[1].transform.x);

        const locked = onRouted.starts[1].transform;
        // Position is the part the lock overrides, scale is the part it never touches.
        // That is the disclosed quirk, and it is shared by both branches.
        expect(onRouted.members[1]).toEqual(onRouted.expected[1]);
        expect(onRouted.members[1].x).toBe(locked.x);
        expect(onRouted.members[1].y).toBe(locked.y);
        expect(onRouted.members[1].scaleX).toBe(onRouted.frames[1].scaleX);
        expect(onRouted.members[1].scaleY).toBe(onRouted.frames[1].scaleY);
        expect(onRouted.members[0]).toEqual(onRouted.expected[0]);
        // Parity across authorities, and the rotation lock has nothing to defend: a
        // resize writes no rotation for either mode on either side.
        expect(onRouted.members).toEqual(onLegacy.members);
        expect(onRouted.members.map((m) => m.rotation)).toEqual(
          onRouted.starts.map((s) => s.transform.rotation),
        );
      } finally {
        legacy.dispose();
        routed.dispose();
      }
    },
  );
});