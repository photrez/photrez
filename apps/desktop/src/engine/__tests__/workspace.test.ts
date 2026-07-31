import { describe, it, expect, vi } from 'vitest';
import { WorkspaceManager } from '../workspace';

describe('WorkspaceManager', () => {
  it('starts with no documents', () => {
    const wm = new WorkspaceManager();
    expect(wm.getDocumentCount()).toBe(0);
    expect(wm.getActiveDocumentId()).toBeNull();
    expect(wm.isFull()).toBe(false);
  });

  it('creates blank document correctly via factory', () => {
    const session = WorkspaceManager.createBlankDocument('doc-1', 'Untitled-1', 400, 300);
    
    expect(session.displayName).toBe('Untitled-1');
    expect(session.engine.getWidth()).toBe(400);
    expect(session.engine.getHeight()).toBe(300);
    expect(session.engine.getLayers().length).toBe(1);
    expect(session.engine.getLayers()[0].name).toBe('Background');
  });

  it('creates Background layer with isBackground and position/rotation locks', () => {
    const session = WorkspaceManager.createBlankDocument('doc-bg-test', 'BG Test', 400, 300);
    const layers = session.engine.getLayers();
    expect(layers).toHaveLength(1);
    expect(layers[0].name).toBe('Background');
    expect(layers[0].isBackground).toBe(true);
    expect(layers[0].lockPosition).toBe(true);
    expect(layers[0].lockRotation).toBe(true);
  });

  it('manages document lifecycle sessions correctly', () => {
    const wm = new WorkspaceManager();
    const session1 = WorkspaceManager.createBlankDocument('doc-1', 'Doc 1', 800, 600);
    const session2 = WorkspaceManager.createBlankDocument('doc-2', 'Doc 2', 400, 300);

    let changeTriggered = 0;
    wm.onChange(() => {
      changeTriggered++;
    });

    wm.addDocument(session1);
    expect(wm.getDocumentCount()).toBe(1);
    expect(wm.getActiveDocumentId()).toBe('doc-1');
    expect(changeTriggered).toBe(1);

    wm.addDocument(session2);
    expect(wm.getDocumentCount()).toBe(2);
    expect(wm.getActiveDocumentId()).toBe('doc-2');

    wm.switchDocument('doc-1');
    expect(wm.getActiveDocumentId()).toBe('doc-1');

    wm.removeDocument('doc-1');
    expect(wm.getDocumentCount()).toBe(1);
    expect(wm.getActiveDocumentId()).toBe('doc-2');
  });

  it('detaches engine callbacks when a document is removed (review #40)', () => {
    const wm = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument('doc-cb', 'CB Test', 400, 300);
    const onChange = vi.fn();
    const onVisualChange = vi.fn();
    session.engine.onChange(onChange);
    session.engine.onVisualChange(onVisualChange);

    wm.addDocument(session);
    wm.removeDocument('doc-cb');

    // Mutating the removed engine must no longer reach the old callbacks.
    session.engine.addLayer('Layer after removal');
    expect(onChange).not.toHaveBeenCalled();
    expect(onVisualChange).not.toHaveBeenCalled();
  });

  // ──────────────────────────────────────────────────────────────
  // DIRTY STATE — UNDO TO CLEAN
  // ──────────────────────────────────────────────────────────────

  describe('dirty state after undo', () => {
    it('undo to clean state resets session.dirty to false (regression 2026-07-06)', () => {
      const ws = new WorkspaceManager();
      const doc = WorkspaceManager.createBlankDocument('doc-dirty-undo', 'Dirty Undo', 800, 600);
      ws.addDocument(doc);
      const engine = doc.engine;
      const layer = engine.addLayer('Layer 1');

      // Clear dirty from addLayer so we start clean
      engine.clearDirty();
      doc.dirty = false;

      // Commit clean initial state
      doc.history.commit(engine.snapshot(), 'Initial');

      expect(engine.isDirty()).toBe(false);
      expect(doc.dirty).toBe(false);

      // Simulate edit: move layer
      engine.moveLayer(layer.id, 100, 50);
      expect(engine.isDirty()).toBe(true);
      expect(doc.dirty).toBe(true);

      // Undo back to initial clean state
      const prev = doc.history.undo(engine.snapshot());
      engine.restore(prev!);

      // After undo, engine should be clean
      expect(engine.isDirty()).toBe(false);
      // session.dirty must also be clean (the bug: onVisualChange always set dirty=true)
      expect(doc.dirty).toBe(false);
    });

    it('undo after brush stroke then undo to clean resets session.dirty', () => {
      const ws = new WorkspaceManager();
      const doc = WorkspaceManager.createBlankDocument('doc-brush-undo', 'Brush Undo', 800, 600);
      ws.addDocument(doc);
      const engine = doc.engine;
      const layer = engine.addLayer('Layer 1', 100, 100);
      const originalBitmap = { width: 100, height: 100 } as ImageBitmap;
      engine.setLayerImageBitmap(layer.id, originalBitmap);
      engine.clearDirty();
      doc.dirty = false;

      // Commit clean state
      doc.history.commit(engine.snapshot(), 'Brush Initial');
      expect(engine.isDirty()).toBe(false);
      expect(doc.dirty).toBe(false);

      // Simulate brush stroke: replace bitmap
      const newBitmap = { width: 100, height: 100 } as ImageBitmap;
      engine.setLayerImageBitmap(layer.id, newBitmap);
      expect(engine.isDirty()).toBe(true);
      expect(doc.dirty).toBe(true);

      // Undo back to original bitmap
      const prev = doc.history.undo(engine.snapshot());
      engine.restore(prev!);

      expect(engine.getLayer(layer.id)!.imageBitmap).toBe(originalBitmap);
      expect(engine.isDirty()).toBe(false);
      expect(doc.dirty).toBe(false);
    });
  });
});
