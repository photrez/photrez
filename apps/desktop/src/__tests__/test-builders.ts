// Typed test builders — reduces `as any` in tests by providing properly typed
// factory functions for engine, history, and tool context mocks.

import { vi } from "vitest";
import type { DocumentEngine } from "../engine/document";
import type { CommandHistory } from "../engine/history";
import type { ToolContext } from "../viewport/input-handler";
import type { DocumentModel, LayerNode } from "../engine/types";
import type { EditorContextValue } from "../components/editor/shell/EditorContext";

// ─── Helpers ────────────────────────────────────────────────────────────

type EngineMethod = keyof DocumentEngine;

/** Default layer returned by createMockEngine's getLayer stub. */
const DEFAULT_LAYER: LayerNode = {
  id: "layer-1",
  name: "Layer 1",
  type: "raster",
  transform: { x: 100, y: 50, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
  locked: false,
  visible: true,
  opacity: 1,
  blendMode: "normal",
  width: 200,
  height: 150,
  imageBitmap: null,
};

/** Creates a mock DocumentEngine with vi.fn() stubs for listed methods.
 *  getLayer returns a default unlocked layer at (100,50).
 *  snapshot returns a minimal empty model. */
export function createMockEngine(methods: EngineMethod[] = ["snapshot", "getLayer", "moveLayer", "moveLayerSilent", "flushChangeNotification", "samplePixel", "createSelection", "clearSelection"]) {
  const stub: Record<string, unknown> = {};
  for (const m of methods) {
    if (m === "getLayer") {
      stub[m] = vi.fn<(...args: never[]) => unknown>().mockReturnValue({ ...DEFAULT_LAYER });
    } else if (m === "snapshot") {
      stub[m] = vi.fn<(...args: never[]) => unknown>().mockReturnValue({} as never);
    } else {
      stub[m] = vi.fn();
    }
  }
  // Selection draw clamping needs document bounds; default to a large canvas
  // so most coordinate-based tests are unaffected unless they override it.
  stub.getWidth = vi.fn<() => number>().mockReturnValue(4000);
  stub.getHeight = vi.fn<() => number>().mockReturnValue(4000);
  return stub as unknown as DocumentEngine;
}

/** Creates a minimal mock CommandHistory. */
export function createMockHistory() {
  return { commit: vi.fn() } as unknown as CommandHistory;
}

/** Creates a typed ToolContext with sensible defaults. */
export function createToolContext(overrides?: Partial<ToolContext>): ToolContext {
  return {
    fgColor: "#000000",
    bgColor: "#FFFFFF",
    brushSize: 20,
    brushHardness: 0.5,
    brushOpacity: 1,
    paintSettings: { size: 20, hardness: 0.5, opacity: 1, flow: 1, smoothing: 0 },
    isAltPressed: false,
    isDragging: false,
    dragStart: { x: 0, y: 0 },
    dragCurrent: { x: 0, y: 0 },
    selectedLayerId: "layer-1",
    strokePoints: [],
    dragTool: null,
    setFgColor: vi.fn(),
    setBgColor: vi.fn(),
    onPaintStroke: undefined,
    onCropCreated: undefined,
    onSelectionCreated: undefined,
    onComputeSnap: undefined,
    onHoverHandle: undefined,
    onSnapLines: vi.fn(),
    ...overrides,
  };
}

/** Wraps a partial useEditor mock. Cast is inside the helper, not at each call site.
 *  Record<string, unknown> avoids deep type-check on nested types like WorkspaceManager.
 *  Callers provide only the properties their test needs. */
export function mockEditorContext(v: Record<string, unknown>): EditorContextValue {
  return v as unknown as EditorContextValue;
}

/** Helper: override a mock engine method's return value with proper typing. */
export function mockEngineMethod<T extends (...args: never[]) => unknown>(
  engine: DocumentEngine,
  method: EngineMethod,
): ReturnType<typeof vi.fn<T>> {
  const fn = (engine as any)[method];
  if (!fn?.mock) throw new Error(`Method ${String(method)} is not a vi.fn() mock`);
  return fn as ReturnType<typeof vi.fn<T>>;
}
