// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * A History-panel jump must NOT step the Rust cursor.
 *
 * `navigateHistory` (EditorContext) is the second pop site in the app: it loops
 * `history.undo()` / `history.redo()` up to `steps` times and then restores with
 * `engine.restore` + `uploadImage` - it never takes pixels from Rust, and it never
 * touches the tile path. Since the Rust cursor step belongs to the POP
 * (`CommandHistory.stepRustCursor`), a pop site that restores through the model
 * has to say so explicitly, or every pop moves a cursor whose pixel bytes never
 * moved.
 *
 * This matters in the SHIPPING default, with no bridge and no
 * `photrez.rustPixels`: `rustOwned` is the default for every brush stroke,
 * because `rust_pixels_write_region` is ungated, so the pop-side predicate
 * reduces to `rustOwned`. A 20-step jump in that state would fire 20 serialised
 * `rust_pixels_undo` round-trips in one click where the old code fired none.
 *
 * The provider is mounted for real (not a double), because the thing under test
 * IS the provider's pop loop.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
import { invoke } from "@tauri-apps/api/core";
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";
import { WorkspaceManager } from "@/engine/workspace";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import type { DocumentModel } from "@/engine/types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));

const DOC = "doc-nav";

const model = (name: string): DocumentModel => ({
  id: DOC,
  name,
  width: 8,
  height: 8,
  layers: [],
  activeLayerId: null,
  selection: null,
  viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
  dirty: false,
});

/**
 * A Rust-owned tile memento: `rustOwned` is what a brush stroke's twin carries,
 * and it is the ONLY arm that arms a step when the bridge is off.
 */
const brushPatches = () => ({
  layerId: "l1",
  surfaceWidth: 8,
  surfaceHeight: 8,
  before: [{ x: 0, y: 0, width: 1, height: 1, data: new Uint8ClampedArray([0, 0, 0, 255]) }],
  after: [{ x: 0, y: 0, width: 1, height: 1, data: new Uint8ClampedArray([9, 9, 9, 255]) }],
  rustOwned: true,
});

const timesInvoked = (command: string): number =>
  vi.mocked(invoke).mock.calls.filter((c) => c[0] === command).length;

const settle = async () => {
  await new Promise<void>((r) => setTimeout(r, 0));
  await flushPixelInvokeCensus();
};

describe("navigateHistory: a model-restore pop takes no Rust cursor step", () => {
  let ws: WorkspaceManager;
  let renderer: Record<string, unknown>;
  let scheduler: Record<string, unknown>;
  let container: HTMLDivElement;
  let dispose: () => void;
  let editorRef: { current: { navigateHistory: (i: number) => void } | null };

  const Capture = () => {
    editorRef.current = useEditor() as never;
    return null;
  };

  beforeEach(async () => {
    // Shipping default: no bridge gate, no photrez.rustPixels, no Tauri runtime.
    localStorage.clear();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "rust_pixels_undo" || cmd === "rust_pixels_redo") {
        return { layer_id: "l1", tiles: [], epoch: 1, version: 1 };
      }
      // Post-image only: the real reply carries no pre-image.
      if (cmd === "rust_pixels_write_region") return { after: [], epoch: 1, version: 1 };
      return undefined;
    });

    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    editorRef = { current: null };

    ws.addDocument(WorkspaceManager.createBlankDocument(DOC, "Nav", 8, 8));

    dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer as never} scheduler={scheduler as never}>
          <Capture />
        </EditorProvider>
      ),
      container,
    );
    await settle();
  });

  afterEach(() => {
    dispose();
    if (container.parentNode) container.parentNode.removeChild(container);
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it("a 3-step jump pops three entries and fires ZERO cursor steps", async () => {
    const session = ws.getActiveSession()!;
    const history = session.history;
    // Three Rust-owned paint entries: the shape that arms a step with the bridge
    // off, so this is the state where an unguarded pop would move the cursor.
    for (let i = 0; i < 3; i++) history.commit(session.engine.snapshot(), "Brush", brushPatches(), true);
    expect(history.getUndoCount()).toBe(3);
    const restore = vi.spyOn(session.engine, "restore");
    const cursorStepInvokes = () => timesInvoked("rust_pixels_undo") + timesInvoked("rust_pixels_redo");

    // Jump to the very first state: three pops in one click.
    editorRef.current!.navigateHistory(0);
    await settle();

    expect(history.getUndoCount(), "three entries popped").toBe(0);
    expect(history.getRedoCount()).toBe(3);
    expect(restore, "it restored through the model").toHaveBeenCalled();
    expect(cursorStepInvokes(), "a model-restore pop must not step the Rust cursor").toBe(0);

    // And back up again: the redo direction must be equally quiet.
    editorRef.current!.navigateHistory(3);
    await settle();
    expect(history.getUndoCount()).toBe(3);
    expect(cursorStepInvokes(), "the redo jump stepped nothing either").toBe(0);
  });

  it("DEFEAT: an unguarded pop DOES fire a step here, so the zero above is real", async () => {
    // Falsifiability. This case runs the same jump against a history whose pops
    // step by default - i.e. with the opt-out removed - and requires the invokes
    // to appear. Without it, the zero in the case above could only mean "this
    // harness never produces a step", which would make it vacuous.
    const session = ws.getActiveSession()!;
    const history = session.history;
    for (let i = 0; i < 3; i++) history.commit(session.engine.snapshot(), "Brush", brushPatches(), true);

    // Call the pop the way an unguarded pop site would: no stepCursor argument.
    const engine = session.engine;
    let snap = engine.snapshot();
    for (let i = 0; i < 3; i++) {
      const next = history.undo(snap);
      if (!next) break;
      snap = next;
    }
    await settle();

    expect(history.getUndoCount()).toBe(0);
    expect(
      timesInvoked("rust_pixels_undo"),
      "three pops, three steps - the behaviour navigateHistory opts out of",
    ).toBe(3);
  });
});
