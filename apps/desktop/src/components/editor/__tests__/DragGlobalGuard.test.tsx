import { describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import {
  DragControllerProvider,
  DragGlobalGuard,
  useDragController,
} from "../DragController";
import type { LayerDragPayload } from "../dragTypes";

/**
 * When an HTML5 drag is in flight, dragover on the
 * document must call preventDefault so the browser doesn't show
 * the "forbidden" cursor over non-drop zones. Without this guard,
 * dragging a layer over the canvas or topbar shows 🚫.
 */
describe("DragGlobalGuard — document-level dragover preventDefault", () => {
  function setup() {
    const container = document.createElement("div");
    document.body.appendChild(container);

    let probe: ReturnType<typeof useDragController> | null = null;
    const dispose = render(
      () => (
        <DragControllerProvider workspaceOverride={{ switchDocument: vi.fn() }}>
          <DragGlobalGuard />
          <ProbeProbe probeRef={(p) => (probe = p)} />
        </DragControllerProvider>
      ),
      container,
    );
    return { probe: () => probe!, dispose };
  }

  function ProbeProbe(props: { probeRef: (p: any) => void }) {
    props.probeRef(useDragController());
    return null;
  }

  function fireDragOverOn(target: EventTarget, type = "dragover") {
    const evt = new Event(type, { bubbles: true, cancelable: true });
    let calledListener = false;
    const capture = () => {
      calledListener = true;
    };
    target.addEventListener(type, capture, { capture: true });
    target.dispatchEvent(evt);
    target.removeEventListener(type, capture, { capture: true });
    if (!calledListener) {
      throw new Error(`No listener fired for ${type}`);
    }
    return evt;
  }

  it("prevents default on document dragover while a layer drag is active", () => {
    const ctx = setup();
    try {
      const payload: LayerDragPayload = {
        version: 1,
        sourceDocId: "doc",
        layerId: "L",
        sourceName: "Layer",
        isAltPressed: false,
      };
      ctx.probe().beginLayerDrag(payload, null);
      // Solid signals: read state to confirm update happened.
      expect(ctx.probe().state().dragKind).toBe("layer");

      const evt = fireDragOverOn(document);
      expect(evt.defaultPrevented).toBe(true);
    } finally {
      ctx.dispose();
      document.body.replaceChildren();
    }
  });

  it("does not prevent default when no drag is active", () => {
    const ctx = setup();
    try {
      // No beginLayerDrag call — dragKind stays null.
      const evt = fireDragOverOn(document);
      expect(evt.defaultPrevented).toBe(false);
    } finally {
      ctx.dispose();
      document.body.replaceChildren();
    }
  });

  it("detects OS file drag and sets dragKind to 'file' with empty paths (Explorer → browser)", () => {
    const ctx = setup();
    try {
      // No layer drag; simulate OS file drag from Explorer/Finder.
      const evt = new Event("dragover", { bubbles: true, cancelable: true }) as any;
      evt.dataTransfer = { types: ["Files"] };
      document.dispatchEvent(evt);

      expect(evt.defaultPrevented).toBe(true);
      expect(ctx.probe().state().dragKind).toBe("file");
      expect(ctx.probe().state().filePaths).toEqual([]);
    } finally {
      ctx.dispose();
      document.body.replaceChildren();
    }
  });

  it("cancels the default drop action for OS file drops outside drop zones", () => {
    const ctx = setup();
    try {
      // OS file drag in flight (drop lands on the topbar / rail / gap —
      // no zone handler runs, so the document-level listener is the only
      // preventDefault. Without it WebView2 would navigate to the file).
      ctx.probe().beginFileDrag([], { x: 0, y: 0 });
      expect(ctx.probe().state().dragKind).toBe("file");

      const evt = new Event("drop", { bubbles: true, cancelable: true }) as any;
      evt.dataTransfer = { files: [new File(["x"], "photo.png")] };
      document.dispatchEvent(evt);

      expect(evt.defaultPrevented).toBe(true);
      // Drop also ends the drag — no orphan state.
      expect(ctx.probe().state().dragKind).toBeNull();
    } finally {
      ctx.dispose();
      document.body.replaceChildren();
    }
  });

  it("removes the document listener on unmount", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    let probe: ReturnType<typeof useDragController> | null = null;
    const dispose = render(
      () => (
        <DragControllerProvider workspaceOverride={{ switchDocument: vi.fn() }}>
          <DragGlobalGuard />
          <ProbeProbe probeRef={(p) => (probe = p)} />
        </DragControllerProvider>
      ),
      container,
    );
    try {
      probe!.beginLayerDrag(
        {
          version: 1,
          sourceDocId: "doc",
          layerId: "L",
          sourceName: "Layer",
          isAltPressed: false,
        },
        null,
      );
      const before = fireDragOverOn(document);
      expect(before.defaultPrevented).toBe(true);

      // Unmount.
      dispose();
      document.body.replaceChildren();

      // After unmount, document listener is gone — dragover should
      // not be preventDefault'd. (We don't have a way to fire on
      // the disposed document, so this just asserts no error.)
      const after = new Event("dragover", { bubbles: true, cancelable: true });
      document.dispatchEvent(after);
      expect(after.defaultPrevented).toBe(false);
    } finally {
      document.body.replaceChildren();
    }
  });
});