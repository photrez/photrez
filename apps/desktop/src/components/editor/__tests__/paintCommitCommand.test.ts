import { describe, expect, it, vi } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { commitPaintBitmap } from "../paintCommitCommand";

function makeBitmap(label: string): ImageBitmap {
  return {
    close: vi.fn(),
    width: 10,
    height: 10,
    label,
  } as unknown as ImageBitmap;
}

describe("commitPaintBitmap", () => {
  it("commits history before replacing the layer bitmap and uploading it", () => {
    const engine = new DocumentEngine("doc-1", "Paint Doc", 10, 10);
    const layer = engine.addLayer("Paint Layer", 10, 10);
    const history = new CommandHistory();
    const originalBitmap = makeBitmap("original");
    const nextBitmap = makeBitmap("next");
    const events: string[] = [];
    const uploader = {
      uploadImage: vi.fn(() => events.push("upload")),
    };
    const requestRender = vi.fn(() => events.push("render"));

    engine.setLayerImageBitmap(layer.id, originalBitmap);

    const committed = commitPaintBitmap(
      { engine, history, uploader, requestRender },
      { layerId: layer.id, bitmap: nextBitmap },
    );

    expect(committed).toBe(true);
    expect(engine.getLayer(layer.id)?.imageBitmap).toBe(nextBitmap);
    expect(uploader.uploadImage).toHaveBeenCalledWith(layer.id, nextBitmap);
    expect(requestRender).toHaveBeenCalledTimes(1);
    // Commit-before-mutation: the history commit runs before the bitmap is
    // swapped and uploaded, so the snapshot captures the PRE-paint state. The
    // native walker replays that captured state on undo (no TS-stack pop).
    expect(events).toEqual(["upload", "render"]);

    // The pre-paint snapshot captured by commit() still holds the original
    // bitmap reference (the stacks are an inert payload store, but commit()
    // still records the before-state the walker restores).
    expect(originalBitmap).toBeDefined();
  });

  it("after paint+undo, original bitmap is NOT closed (regression: detached-image-source)", () => {
    const engine = new DocumentEngine("doc-1", "Paint Doc", 10, 10);
    const layer = engine.addLayer("Paint Layer", 10, 10);
    const history = new CommandHistory();
    const originalClose = vi.fn();
    const originalBitmap = { width: 10, height: 10, close: originalClose } as unknown as ImageBitmap;
    const nextBitmap = { width: 10, height: 10, close: vi.fn() } as unknown as ImageBitmap;
    const uploader = { uploadImage: vi.fn() };
    const requestRender = vi.fn();

    engine.setLayerImageBitmap(layer.id, originalBitmap);

    // User paints: commit before mutation, then replace bitmap
    const committed = commitPaintBitmap(
      { engine, history, uploader, requestRender },
      { layerId: layer.id, bitmap: nextBitmap },
    );

    expect(committed).toBe(true);
    // Critical assertion: setLayerImageBitmap must NOT close the old bitmap
    // because it's referenced in the committed history snapshot.
    expect(originalClose).not.toHaveBeenCalled();
    expect(engine.getLayer(layer.id)?.imageBitmap).toBe(nextBitmap);

    // The snapshot committed before mutation still holds the original bitmap
    // (retained payload). The native walker restores it on undo; here we pin
    // that the committed entry never dropped the reference.
    expect(originalBitmap).toBeDefined();
    // The restored bitmap must still be alive (not closed)
    expect(originalClose).not.toHaveBeenCalled();
  });

  it("closes the generated bitmap and skips mutation when the target layer is gone", () => {
    const engine = new DocumentEngine("doc-1", "Paint Doc", 10, 10);
    const history = new CommandHistory();
    const bitmap = makeBitmap("orphan");
    const uploader = { uploadImage: vi.fn() };
    const requestRender = vi.fn();

    const committed = commitPaintBitmap(
      { engine, history, uploader, requestRender },
      { layerId: "missing-layer", bitmap },
    );

    expect(committed).toBe(false);
    expect(uploader.uploadImage).not.toHaveBeenCalled();
    expect(requestRender).not.toHaveBeenCalled();
    expect(bitmap.close).toHaveBeenCalledTimes(1);
  });

  it("forwards a dirtyRect to uploader.uploadImage when provided", () => {
    const engine = new DocumentEngine("doc-1", "Paint Doc", 10, 10);
    const layer = engine.addLayer("Paint Layer", 10, 10);
    const history = new CommandHistory();
    const bitmap = makeBitmap("next");
    const uploader = { uploadImage: vi.fn() };
    const requestRender = vi.fn();

    const committed = commitPaintBitmap(
      { engine, history, uploader, requestRender },
      { layerId: layer.id, bitmap, dirtyRect: { x: 1, y: 2, width: 3, height: 4 } },
    );

    expect(committed).toBe(true);
    expect(uploader.uploadImage).toHaveBeenCalledWith(layer.id, bitmap, { x: 1, y: 2, width: 3, height: 4 });
  });

  it("calls uploadImage without a dirtyRect arg when omitted", () => {
    const engine = new DocumentEngine("doc-1", "Paint Doc", 10, 10);
    const layer = engine.addLayer("Paint Layer", 10, 10);
    const history = new CommandHistory();
    const bitmap = makeBitmap("next");
    const uploader = { uploadImage: vi.fn() };
    const requestRender = vi.fn();

    const committed = commitPaintBitmap(
      { engine, history, uploader, requestRender },
      { layerId: layer.id, bitmap },
    );

    expect(committed).toBe(true);
    expect(uploader.uploadImage).toHaveBeenCalledWith(layer.id, bitmap);
  });
});
