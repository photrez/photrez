import { describe, it, expect, vi, afterEach } from 'vitest';
import { DocumentEngine } from '../document';
import { setDeviceMaxTextureSize, MAX_CANVAS_DIM } from '../types';

describe('DocumentEngine', () => {
  it('creates document with correct dimensions and initial states', () => {
    const engine = new DocumentEngine('doc-1', 'My Document', 800, 600);
    
    expect(engine.getId()).toBe('doc-1');
    expect(engine.getName()).toBe('My Document');
    expect(engine.getWidth()).toBe(800);
    expect(engine.getHeight()).toBe(600);
    expect(engine.getLayers().length).toBe(0);
    expect(engine.getActiveLayerId()).toBeNull();
    expect(engine.getSelection()).toBeNull();
    expect(engine.isDirty()).toBe(false);
  });

  it('clearDirty resets dirty flag and mutation re-sets it', () => {
    const engine = new DocumentEngine('doc-dirty', 'Dirty Flag', 800, 600);
    expect(engine.isDirty()).toBe(false);

    // After any mutation, dirty should be true
    engine.addLayer('Test Layer');
    expect(engine.isDirty()).toBe(true);

    // clearDirty resets it
    engine.clearDirty();
    expect(engine.isDirty()).toBe(false);

    // Another mutation re-sets it
    const layer = engine.addLayer('Layer 2');
    engine.moveLayer(layer.id, 50, 50);
    expect(engine.isDirty()).toBe(true);

    // clearDirty again
    engine.clearDirty();
    expect(engine.isDirty()).toBe(false);
  });

  it('save then undo to a different (pre-save) state stays dirty [regression: was clean]', () => {
    const engine = new DocumentEngine('doc-savedirty', 'Save Dirty', 800, 600);
    engine.addLayer('Layer 1');
    const l2 = engine.addLayer('Layer 2');
    engine.clearDirty(); // "save" baseline = [Layer 1, Layer 2]

    // Edit: remove Layer 2 (the "edit" after save)
    const edited = engine.snapshot();
    edited.layers = edited.layers.filter(l => l.id !== l2.id);
    engine.restore(edited);
    expect(engine.isDirty()).toBe(true); // differs from saved baseline

    // Undo further to the pre-save state (also [Layer 1] only) — still
    // differs from the [Layer1, Layer2] saved baseline -> must stay dirty
    const preSave = engine.snapshot();
    preSave.layers = preSave.layers.filter(l => l.id !== l2.id);
    engine.restore(preSave);
    expect(engine.getLayers().some(l => l.id === l2.id)).toBe(false);
    expect(engine.isDirty()).toBe(true);
  });

  it('save then undo exactly to the saved baseline is clean', () => {
    const engine = new DocumentEngine('doc-savedclean', 'Save Clean', 800, 600);
    engine.addLayer('Layer 1');
    engine.clearDirty(); // saved baseline = [Layer 1]

    // Edit then undo back to the exact saved baseline
    const l2 = engine.addLayer('Layer 2');
    expect(engine.isDirty()).toBe(true);
    engine.restore(engine.snapshot()); // no-op-ish restore of current
    // remove l2 so we match the saved baseline exactly
    const baseline = engine.snapshot();
    baseline.layers = baseline.layers.filter(l => l.id !== l2.id);
    engine.restore(baseline);
    expect(engine.isDirty()).toBe(false);
  });

  it('adds layers and updates layer list', () => {
    const engine = new DocumentEngine('doc-1', 'My Document', 800, 600);
    
    const layer1 = engine.addLayer('Background');
    expect(engine.getLayers().length).toBe(1);
    expect(engine.getActiveLayerId()).toBe(layer1.id);
    expect(engine.getLayers()[0].name).toBe('Background');

    const layer2 = engine.addLayer('Layer 1');
    expect(engine.getLayers().length).toBe(2);
    expect(engine.getActiveLayerId()).toBe(layer2.id);
    expect(engine.getLayers()[0].name).toBe('Layer 1'); // Inserted at top
    expect(engine.getLayers()[1].name).toBe('Background');
  });

  it('deletes layer and adjusts active layer selection', () => {
    const engine = new DocumentEngine('doc-1', 'My Document', 800, 600);
    
    const layer1 = engine.addLayer('Layer 1');
    const layer2 = engine.addLayer('Layer 2');
    
    expect(engine.getLayers().length).toBe(2);
    expect(engine.getActiveLayerId()).toBe(layer2.id);

    engine.deleteLayer(layer2.id);
    expect(engine.getLayers().length).toBe(1);
    expect(engine.getActiveLayerId()).toBe(layer1.id);

    // Prevent deleting the last layer
    engine.deleteLayer(layer1.id);
    expect(engine.getLayers().length).toBe(1);
  });

  it('reorders layers list correctly', () => {
    const engine = new DocumentEngine('doc-1', 'My Document', 800, 600);
    
    const l1 = engine.addLayer('L1'); // index 2
    const l2 = engine.addLayer('L2'); // index 1
    const l3 = engine.addLayer('L3'); // index 0

    expect(engine.getLayers().map(l => l.name)).toEqual(['L3', 'L2', 'L1']);

    engine.reorderLayer(0, 2); // Move L3 to end
    expect(engine.getLayers().map(l => l.name)).toEqual(['L2', 'L1', 'L3']);
  });

  it('sets layer properties correctly', () => {
    const engine = new DocumentEngine('doc-1', 'My Document', 800, 600);
    const layer = engine.addLayer('Test');

    engine.setLayerOpacity(layer.id, 0.7);
    expect(engine.getLayers()[0].opacity).toBe(0.7);

    engine.setLayerVisibility(layer.id, false);
    expect(engine.getLayers()[0].visible).toBe(false);

    engine.setLayerLocked(layer.id, true);
    expect(engine.getLayers()[0].locked).toBe(true);

    // When locked, opacity/visibility mutations should be blocked
    engine.setLayerOpacity(layer.id, 0.2);
    expect(engine.getLayers()[0].opacity).toBe(0.7);
  });

  it('performs layer transformations correctly', () => {
    const engine = new DocumentEngine('doc-1', 'My Document', 800, 600);
    const layer = engine.addLayer('Test');

    engine.moveLayer(layer.id, 50, -30);
    expect(engine.getLayers()[0].transform.x).toBe(50);
    expect(engine.getLayers()[0].transform.y).toBe(-30);

    engine.transformLayer(layer.id, { scaleX: 2.0, rotation: 45 });
    expect(engine.getLayers()[0].transform.scaleX).toBe(2.0);
    expect(engine.getLayers()[0].transform.rotation).toBe(45);

    engine.flipLayer(layer.id, 'h');
    expect(engine.getLayers()[0].transform.flipH).toBe(true);
  });

  it('manages selection state correctly', () => {
    const engine = new DocumentEngine('doc-1', 'My Document', 800, 600);
    
    engine.createSelection(10, 20, 100, 200);
    expect(engine.getSelection()).toEqual({ x: 10, y: 20, width: 100, height: 200, angle: 0 });

    engine.selectAll();
    expect(engine.getSelection()).toEqual({ x: 0, y: 0, width: 800, height: 600, angle: 0 });

    engine.clearSelection();
    expect(engine.getSelection()).toBeNull();
  });

  it('invertSelection toggles the complement of the current bounds', () => {
    const engine = new DocumentEngine('doc-1', 'My Document', 800, 600);

    engine.createSelection(10, 20, 100, 200);
    expect(engine.getSelection()).toEqual({ x: 10, y: 20, width: 100, height: 200, angle: 0 });

    engine.invertSelection();
    expect(engine.getSelection()).toEqual({
      x: 10, y: 20, width: 100, height: 200, angle: 0, inverted: true,
    });

    engine.invertSelection();
    expect(engine.getSelection()).toEqual({
      x: 10, y: 20, width: 100, height: 200, angle: 0, inverted: false,
    });
  });

  it('invertSelection selects the full document when there is no selection', () => {
    const engine = new DocumentEngine('doc-1', 'My Document', 800, 600);
    engine.invertSelection();
    expect(engine.getSelection()).toEqual({ x: 0, y: 0, width: 800, height: 600, angle: 0 });
  });

  it('performs snapshot and restore round-trips correctly', () => {
    const engine = new DocumentEngine('doc-1', 'My Document', 800, 600);
    engine.addLayer('Layer 1');
    engine.addLayer('Layer 2');

    const snap = engine.snapshot();
    expect(snap.layers.length).toBe(2);

    engine.addLayer('Layer 3');
    expect(engine.getLayers().length).toBe(3);

    engine.restore(snap);
    expect(engine.getLayers().length).toBe(2);
    expect(engine.getLayers().map(l => l.name)).toEqual(['Layer 2', 'Layer 1']);
  });

  it('snapshot creates deep clone — model mutation does not affect snapshot', () => {
    const engine = new DocumentEngine('doc-deep', 'Deep Clone', 800, 600);
    const l1 = engine.addLayer('Layer 1');
    l1.transform.x = 100;

    const snap = engine.snapshot();

    engine.setViewport({ panX: 999, panY: 999, zoom: 5, rotation: 0 });
    engine.getLayer(l1.id)!.transform.x = 200;
    engine.getLayer(l1.id)!.opacity = 0.1;

    expect(snap.viewport.panX).toBe(0);
    expect(snap.layers[0].transform.x).toBe(100);
    expect(snap.layers[0].opacity).toBe(1);
  });

  it('snapshot with selection is deep-cloned', () => {
    const engine = new DocumentEngine('doc-sel', 'Selection', 800, 600);
    engine.addLayer('Layer 1');
    engine.createSelection(10, 20, 100, 200);

    const snap = engine.snapshot();
    expect(snap.selection).toEqual({ x: 10, y: 20, width: 100, height: 200, angle: 0 });

    engine.clearSelection();
    expect(snap.selection).toEqual({ x: 10, y: 20, width: 100, height: 200, angle: 0 });
  });

  it('restore replaces all mutable state', () => {
    const engine = new DocumentEngine('doc-restore', 'Restore', 400, 300);
    engine.addLayer('Layer 1');
    engine.setViewport({ panX: 100, panY: 50, zoom: 2, rotation: 0 });
    engine.createSelection(0, 0, 400, 300);

    const snap = engine.snapshot();

    engine.addLayer('Layer 2');
    engine.setViewport({ panX: 0, panY: 0, zoom: 1, rotation: 0 });
    engine.clearSelection();

    engine.restore(snap, { restoreViewport: true });

    expect(engine.getLayers().length).toBe(1);
    expect(engine.getViewport()).toEqual({ panX: 100, panY: 50, zoom: 2, rotation: 0 });
    expect(engine.getSelection()).toEqual({ x: 0, y: 0, width: 400, height: 300, angle: 0 });
  });

  it('restore handles selection of null', () => {
    const engine = new DocumentEngine('doc-null-sel', 'No Sel', 800, 600);
    engine.addLayer('Layer 1');
    const snap = engine.snapshot();

    expect(snap.selection).toBeNull();
    engine.restore(snap);
    expect(engine.getSelection()).toBeNull();
  });

  describe('zoomToSelection', () => {
    const W = 800;
    const H = 600;

    it('fits the selection rect into the viewport (centered) when a selection exists', () => {
      const engine = new DocumentEngine('doc-zts', 'ZTS', W, H);
      engine.addLayer('Layer 1');
      engine.createSelection(100, 100, 200, 200); // 200x200 at (100,100)

      engine.zoomToSelection(W, H);
      const vp = engine.getViewport();

      // Expected: zoom = min((800-80)/200, (600-80)/200, 10) = 2.6
      expect(vp.zoom).toBeCloseTo(2.6, 5);
      // Selection center (200,200) must map to viewport center (400,300):
      // panX = (800 - 200*2.6)/2 - 100*2.6 = -120
      // panY = (600 - 200*2.6)/2 - 100*2.6 = -220
      expect(vp.panX).toBeCloseTo(-120, 1);
      expect(vp.panY).toBeCloseTo(-220, 1);
    });

    it('clamps the selection rect to document bounds', () => {
      const engine = new DocumentEngine('doc-zts-clamp', 'ZTS', W, H);
      engine.addLayer('Layer 1');
      engine.createSelection(-50, -50, 900, 700); // extends beyond doc

      engine.zoomToSelection(W, H);
      const vp = engine.getViewport();
      // Clamped rect = full document -> same as fitToScreen (zoom = 520/600 = 0.8666...)
      expect(vp.zoom).toBeCloseTo(520 / 600, 5);
    });

    it('falls back to fitToScreen when no selection exists', () => {
      const engine = new DocumentEngine('doc-zts-none', 'ZTS', W, H);
      engine.addLayer('Layer 1');

      engine.zoomToSelection(W, H);
      const vp = engine.getViewport();
      // Whole-doc fit: zoom = min((800-80)/800, (600-80)/600, 10) = 0.8666..., centered
      expect(vp.zoom).toBeCloseTo(520 / 600, 5);
      expect(vp.panX).toBeCloseTo((W - W * vp.zoom) / 2, 1);
    });
  });

  it('multiple snapshots remain independent after restore', () => {
    const engine = new DocumentEngine('doc-multi', 'Multi', 800, 600);
    engine.addLayer('A');

    const snap1 = engine.snapshot();
    engine.addLayer('B');
    const snap2 = engine.snapshot();

    engine.restore(snap1);
    expect(engine.getLayers().map(l => l.name)).toEqual(['A']);

    engine.restore(snap2);
    expect(engine.getLayers().map(l => l.name)).toEqual(['B', 'A']);
  });

  it('snapshot reuses ImageBitmap reference (immutable)', () => {
    const bitmap = { width: 100, height: 100 } as ImageBitmap;
    const engine = new DocumentEngine('doc-bitmap', 'Bitmap', 800, 600);
    const l1 = engine.addLayer('Layer 1', 100, 100);
    engine.setLayerImageBitmap(l1.id, bitmap);

    const snap = engine.snapshot();
    expect(snap.layers[0].imageBitmap).toBe(bitmap);

    const newBitmap = { width: 200, height: 200 } as ImageBitmap;
    engine.setLayerImageBitmap(l1.id, newBitmap);

    expect(snap.layers[0].imageBitmap).toBe(bitmap);
    expect(snap.layers[0].imageBitmap).not.toBe(newBitmap);
  });

  it('restore preserves ImageBitmap reference from snapshot', () => {
    const bitmap = { width: 100, height: 100 } as ImageBitmap;
    const engine = new DocumentEngine('doc-restore-bmp', 'Restore Bitmap', 800, 600);
    const l1 = engine.addLayer('Layer 1', 100, 100);
    engine.setLayerImageBitmap(l1.id, bitmap);

    const snap = engine.snapshot();
    const newBitmap = { width: 200, height: 200 } as ImageBitmap;
    engine.setLayerImageBitmap(l1.id, newBitmap);

    engine.restore(snap);
    expect(engine.getLayer(l1.id)!.imageBitmap).toBe(bitmap);
  });

  it('restore does not affect previously taken snapshots', () => {
    const engine = new DocumentEngine('doc-indep', 'Independent', 800, 600);
    engine.addLayer('V1');
    const snap1 = engine.snapshot();

    engine.addLayer('V2');
    const snap2 = engine.snapshot();

    engine.restore(snap1);
    expect(snap2.layers.length).toBe(2);
  });

  describe('getRenderState', () => {
    it('returns documentSize matching engine dimensions (not canvas pixel buffer)', () => {
      // Bug regression test: getRenderState() must use the document's intrinsic
      // dimensions, never the canvas pixel buffer. The renderer relies on
      // documentSize to build the orthographic view projection — feeding it
      // canvas dimensions (which can be docW × zoom × dpr) breaks the fit.
      const engine = new DocumentEngine('doc-render', 'Render Test', 1920, 1280);
      engine.addLayer('Background');

      const state = engine.getRenderState();

      expect(state.documentSize.width).toBe(1920);
      expect(state.documentSize.height).toBe(1280);
      // Ensure these do NOT match a hypothetical canvas pixel buffer
      // (e.g., 1920 × 1 × 1.25 = 2400 for HiDPI)
      expect(state.documentSize.width).not.toBe(2400);
      expect(state.documentSize.height).not.toBe(1600);
    });

    it('reflects layer transforms and metadata in render state', () => {
      const engine = new DocumentEngine('doc-render-2', 'Render Test 2', 800, 600);
      const layer = engine.addLayer('Subject');
      layer.transform.x = 100;
      layer.transform.y = 50;
      layer.opacity = 0.75;

      const state = engine.getRenderState();

      expect(state.layers.length).toBe(1);
      expect(state.layers[0].id).toBe(layer.id);
      expect(state.layers[0].transform.x).toBe(100);
      expect(state.layers[0].transform.y).toBe(50);
      expect(state.layers[0].opacity).toBe(0.75);
      expect(state.documentSize).toEqual({ width: 800, height: 600 });
    });

    it('exposes viewport state for the renderer to consume', () => {
      const engine = new DocumentEngine('doc-render-3', 'Render Test 3', 400, 300);
      engine.setViewport({ panX: 25, panY: -10, zoom: 1.5 });

      const state = engine.getRenderState();

      expect(state.viewport.panX).toBe(25);
      expect(state.viewport.panY).toBe(-10);
      expect(state.viewport.zoom).toBe(1.5);
    });

    it('enables the checkerboard flag so transparent pixels are visible (regression)', () => {
      // Bug regression: getRenderState() must expose checkerboard: true so the
      // WebGL renderer paints the chessboard pattern behind transparent pixels.
      // If this is false, the user cannot tell where transparency is.
      const engine = new DocumentEngine('doc-checker', 'Checker', 800, 600);
      engine.addLayer('Layer 1');

      const state = engine.getRenderState();

      expect(state.checkerboard).toBe(true);
    });

    it('exposes backgroundColor as a 4-tuple RGBA in [0..1]', () => {
      const engine = new DocumentEngine('doc-bg', 'BG', 800, 600);

      const state = engine.getRenderState();

      expect(state.backgroundColor).toHaveLength(4);
      for (const channel of state.backgroundColor) {
        expect(channel).toBeGreaterThanOrEqual(0);
        expect(channel).toBeLessThanOrEqual(1);
      }
      expect(state.backgroundColor[3]).toBe(1); // alpha always opaque
    });

    it('hides exactly the render-hidden layer while others stay visible', () => {
      const engine = new DocumentEngine('doc-hidden', 'Hidden', 800, 600);
      const a = engine.addLayer('A');
      const b = engine.addLayer('B');

      engine.setRenderHiddenLayerId(a.id);
      const state = engine.getRenderState();
      expect(state.layers.find((l) => l.id === a.id)!.visible).toBe(false);
      expect(state.layers.find((l) => l.id === b.id)!.visible).toBe(true);

      // Clearing the hidden id restores visibility (session close).
      engine.setRenderHiddenLayerId(null);
      const after = engine.getRenderState();
      expect(after.layers.find((l) => l.id === a.id)!.visible).toBe(true);
    });

    it('a hidden layer that is already invisible stays invisible', () => {
      const engine = new DocumentEngine('doc-hidden-2', 'Hidden 2', 800, 600);
      const a = engine.addLayer('A');
      a.visible = false;
      engine.setRenderHiddenLayerId(a.id);
      const state = engine.getRenderState();
      expect(state.layers.find((l) => l.id === a.id)!.visible).toBe(false);
    });
  });

  describe('applyCrop', () => {
    it('offsets layers and resizes document (non-destructive)', () => {
      const engine = new DocumentEngine('doc-crop', 'Crop', 800, 600);
      const layer = engine.addLayer('Layer 1');
      layer.transform.x = 100;
      layer.transform.y = 50;

      engine.applyCrop(50, 30, 400, 300);

      expect(engine.getWidth()).toBe(400);
      expect(engine.getHeight()).toBe(300);
      expect(layer.transform.x).toBe(50);
      expect(layer.transform.y).toBe(20);
    });

    it('does nothing with zero dimensions', () => {
      const engine = new DocumentEngine('doc-crop-zero', 'Crop', 800, 600);
      engine.addLayer('Layer 1');

      engine.applyCrop(0, 0, 0, 0);

      expect(engine.getWidth()).toBe(800);
      expect(engine.getHeight()).toBe(600);
    });

    it('applies targetSize scaling', () => {
      const engine = new DocumentEngine('doc-crop-size', 'Crop Size', 800, 600);
      const layer = engine.addLayer('Layer 1');
      layer.transform.x = 200;
      layer.transform.y = 100;

      engine.applyCrop(100, 50, 400, 300, { targetSize: { w: 800, h: 600 } });

      expect(engine.getWidth()).toBe(800);
      expect(engine.getHeight()).toBe(600);
      // Layer transform scaled by 800/400 = 2x, 600/300 = 2x
      expect(layer.transform.x).toBeCloseTo(200, 0);
      expect(layer.transform.y).toBeCloseTo(100, 0);
    });

    it('applies independent targetSize scaling on each axis', () => {
      const engine = new DocumentEngine('doc-crop-size-nonuniform', 'Crop Size Nonuniform', 800, 600);
      const layer = engine.addLayer('Layer 1', 100, 100);
      layer.transform.x = 200;
      layer.transform.y = 100;

      engine.applyCrop(100, 50, 400, 300, { targetSize: { w: 800, h: 300 } });

      expect(engine.getWidth()).toBe(800);
      expect(engine.getHeight()).toBe(300);
      expect(layer.transform.x).toBeCloseTo(200, 4);
      expect(layer.transform.y).toBeCloseTo(50, 4);
      expect(layer.transform.scaleX).toBeCloseTo(2, 4);
      expect(layer.transform.scaleY).toBeCloseTo(1, 4);
    });

    it('does not move locked layers (non-destructive)', () => {
      const engine = new DocumentEngine('doc-crop-lock', 'Crop', 800, 600);
      const layer = engine.addLayer('Layer 1');
      layer.locked = true;
      layer.transform.x = 100;
      layer.transform.y = 50;

      engine.applyCrop(50, 30, 400, 300);

      expect(layer.transform.x).toBe(100);
      expect(layer.transform.y).toBe(50);
    });

    it('offsets layers in deleteCroppedPixels mode even without bitmap', () => {
      const engine = new DocumentEngine('doc-crop-del', 'Crop Del', 200, 200);
      const layer = engine.addLayer('Layer 1');
      layer.transform.x = 100;
      layer.transform.y = 50;

      engine.applyCrop(50, 50, 100, 100, { deleteCroppedPixels: true });

      expect(engine.getWidth()).toBe(100);
      expect(engine.getHeight()).toBe(100);
      expect(layer.transform.x).toBe(50);
      expect(layer.transform.y).toBe(0);
    });

    it('transforms layers based on crop rotation', () => {
      const engine = new DocumentEngine('doc-crop-rot', 'Crop Rotation', 800, 600);
      const layer = engine.addLayer('Layer 1', 100, 100);
      layer.transform.x = 200;
      layer.transform.y = 150;
      layer.transform.rotation = 10;

      engine.applyCrop(200, 150, 400, 300, { rotation: 90 });

      expect(engine.getWidth()).toBe(400);
      expect(engine.getHeight()).toBe(300);
      expect(layer.transform.x).toBeCloseTo(50, 4);
      expect(layer.transform.y).toBeCloseTo(250, 4);
      expect(layer.transform.rotation).toBeCloseTo(-80, 4);
    });
  });

  describe('layer overhaul additions', () => {
    it('inserts new layer above the active layer', () => {
      const engine = new DocumentEngine('doc-add', 'Add Contextual', 800, 600);
      const l1 = engine.addLayer('Layer 1');
      const l2 = engine.addLayer('Layer 2');
      const l3 = engine.addLayer('Layer 3');
      // Visual stack: L3, L2, L1 (indices: 0, 1, 2)
      expect(engine.getLayers().map(l => l.name)).toEqual(['Layer 3', 'Layer 2', 'Layer 1']);

      // Select Layer 2 (index 1)
      engine.setActiveLayer(l2.id);

      // Add a new layer
      const lNew = engine.addLayer('Layer New');
      // Should be inserted at index 1 (above Layer 2, shifting Layer 2 to index 2)
      expect(engine.getLayers().map(l => l.name)).toEqual(['Layer 3', 'Layer New', 'Layer 2', 'Layer 1']);
      expect(engine.getActiveLayerId()).toBe(lNew.id);
    });

    it('duplicates a layer correctly', () => {
      const engine = new DocumentEngine('doc-dup', 'Duplicate', 800, 600);
      const l1 = engine.addLayer('Layer 1');
      const l2 = engine.addLayer('Layer 2');

      l2.opacity = 0.5;
      l2.blendMode = 'multiply';
      l2.transform.x = 20;

      const dup = engine.duplicateLayer(l2.id);
      
      expect(dup.name).toBe('Layer 3');
      expect(dup.opacity).toBe(0.5);
      expect(dup.blendMode).toBe('multiply');
      expect(dup.transform.x).toBe(20);
      
      // Duplicated layer should be directly above l2
      const names = engine.getLayers().map(l => l.name);
      expect(names).toEqual(['Layer 3', 'Layer 2', 'Layer 1']);
      expect(engine.getActiveLayerId()).toBe(dup.id);
    });

    it('merges active layer down correctly', () => {
      const engine = new DocumentEngine('doc-merge', 'Merge Down', 800, 600);
      const l1 = engine.addLayer('Layer 1');
      const l2 = engine.addLayer('Layer 2');
      
      // Visual stack: L2, L1
      expect(engine.getLayers().map(l => l.name)).toEqual(['Layer 2', 'Layer 1']);
      
      engine.setActiveLayer(l2.id);
      engine.mergeDown(l2.id);
      
      expect(engine.getLayers().length).toBe(1);
      expect(engine.getLayers()[0].name).toBe('Layer 2 + Layer 1');
      expect(engine.getActiveLayerId()).toBe(engine.getLayers()[0].id);
    });

    it('flattens layers list correctly', () => {
      const engine = new DocumentEngine('doc-flat', 'Flatten', 800, 600);
      engine.addLayer('Layer 1');
      engine.addLayer('Layer 2');
      engine.addLayer('Layer 3');

      expect(engine.getLayers().length).toBe(3);

      engine.flattenLayers();

      expect(engine.getLayers().length).toBe(1);
      expect(engine.getLayers()[0].name).toBe('Background');
      expect(engine.getLayers()[0].transform.x).toBe(0);
      expect(engine.getLayers()[0].transform.scaleX).toBe(1.0);
    });
  });

  describe('ImageBitmap lifecycle', () => {
    // Minimal mock with the two methods our code touches.
    function makeBitmap(): ImageBitmap {
      const close = vi.fn();
      const bitmap = { width: 100, height: 100, close } as unknown as ImageBitmap;
      return bitmap;
    }

    it('does NOT close bitmaps when deleteLayer removes its layer (snapshot safety)', () => {
      const engine = new DocumentEngine('doc', 'D', 800, 600);
      // Start with two layers so we can delete one and still have
      // another standing (the engine refuses to delete the last).
      engine.addLayer('Background', 100, 100);
      const target = engine.addLayer('Target', 100, 100);
      const other = engine.addLayer('Other', 100, 100);
      const bitmap = makeBitmap();
      engine.setLayerImageBitmap(target.id, bitmap);
      engine.deleteLayer(other.id); // unrelated — should not close anything
      expect(bitmap.close).not.toHaveBeenCalled();
      engine.deleteLayer(target.id);
      // Bitmaps are intentionally NOT closed — undo stack snapshots
      // may hold a reference to them.
      expect(bitmap.close).not.toHaveBeenCalled();
    });

    it('does NOT close the previous bitmap when setLayerImageBitmap replaces it (snapshot safety)', () => {
      const engine = new DocumentEngine('doc', 'D', 800, 600);
      const l1 = engine.addLayer('L1', 100, 100);
      const oldBitmap = makeBitmap();
      const newBitmap = makeBitmap();
      engine.setLayerImageBitmap(l1.id, oldBitmap);
      engine.setLayerImageBitmap(l1.id, newBitmap);
      // oldBitmap is intentionally NOT closed — undo stack snapshots
      // hold a reference to it.
      expect(oldBitmap.close).not.toHaveBeenCalled();
      expect(newBitmap.close).not.toHaveBeenCalled();
    });

    it('does not close when setLayerImageBitmap is called with the same reference', () => {
      const engine = new DocumentEngine('doc', 'D', 800, 600);
      const l1 = engine.addLayer('L1', 100, 100);
      const bitmap = makeBitmap();
      engine.setLayerImageBitmap(l1.id, bitmap);
      engine.setLayerImageBitmap(l1.id, bitmap);
      expect(bitmap.close).not.toHaveBeenCalled();
    });
  });

  describe('Dimension bounds', () => {
    afterEach(() => {
      setDeviceMaxTextureSize(MAX_CANVAS_DIM); // reset
    });

    it('resizeCanvas clamps to MAX_CANVAS_DIM by default', () => {
      const engine = new DocumentEngine('doc', 'D', 800, 600);
      engine.resizeCanvas(99999, 99999);
      // Should be silently rejected — dimensions unchanged
      expect(engine.getWidth()).toBe(800);
      expect(engine.getHeight()).toBe(600);
    });

    it('resizeCanvas clamps to device-adaptive limit when lower', () => {
      setDeviceMaxTextureSize(4096);
      const engine = new DocumentEngine('doc', 'D', 800, 600);
      engine.resizeCanvas(5000, 5000);
      // 5000 > 4096 → rejected
      expect(engine.getWidth()).toBe(800);
      expect(engine.getHeight()).toBe(600);
    });

    it('resizeCanvas accepts dimensions within limit', () => {
      const engine = new DocumentEngine('doc', 'D', 800, 600);
      engine.resizeCanvas(1024, 768);
      expect(engine.getWidth()).toBe(1024);
      expect(engine.getHeight()).toBe(768);
    });

    it('addLayer throws when dimensions exceed device limit', () => {
      setDeviceMaxTextureSize(4096);
      const engine = new DocumentEngine('doc', 'D', 800, 600);
      expect(() => engine.addLayer('Huge', 5000, 5000)).toThrow(/device limit/);
    });

    it('addLayer accepts dimensions within device limit', () => {
      setDeviceMaxTextureSize(4096);
      const engine = new DocumentEngine('doc', 'D', 800, 600);
      expect(() => engine.addLayer('OK', 4096, 4096)).not.toThrow();
    });
  });

  describe('Background layer', () => {
    it('does not delete the Background layer', () => {
      const engine = new DocumentEngine('doc-bg-test', 'BG Test', 800, 600);
      const bg = engine.addLayer('Background');
      bg.isBackground = true;
      bg.lockPosition = true;
      bg.lockRotation = true;

      engine.deleteLayer(bg.id);
      const layers = engine.getLayers();
      expect(layers.some(l => l.id === bg.id)).toBe(true);
      expect(engine.getLayers()).toHaveLength(1);
    });

    it('keeps the Background layer at the bottom of the stack', () => {
      const engine = new DocumentEngine('doc-bg-test', 'BG Test', 800, 600);
      const bg = engine.addLayer('Background');
      bg.isBackground = true;
      bg.lockPosition = true;
      bg.lockRotation = true;
      const l1 = engine.addLayer('Layer 1'); // -> [Layer 1, Background]

      // "Send to back" the normal layer: it must land just above the
      // Background, never beneath it (which would hide it).
      engine.reorderLayer(0, 1);
      const layers = engine.getLayers();
      expect(layers[layers.length - 1].id).toBe(bg.id); // bg stays bottom
      expect(layers[layers.length - 2].id).toBe(l1.id); // layer above bg
    });

    it('prevents a normal layer from being hidden below the Background (send to back)', () => {
      const engine = new DocumentEngine('doc-bg-bug', 'BG Test', 800, 600);
      const bg = engine.addLayer('Background');
      bg.isBackground = true;
      const l1 = engine.addLayer('Layer 1'); // [Layer 1, Background]
      const l2 = engine.addLayer('Layer 2'); // [Layer 2, Layer 1, Background]

      // Simulate "Send To Back" (Ctrl+Shift+[) on the top layer.
      engine.reorderLayer(0, engine.getLayers().length - 1);

      const layers = engine.getLayers();
      expect(layers[layers.length - 1].id).toBe(bg.id); // bg bottom
      expect(layers[layers.length - 2].id).toBe(l2.id); // top layer just above bg
      expect(layers.findIndex((l) => l.id === l1.id))
        .toBeLessThan(layers.findIndex((l) => l.id === bg.id)); // l1 above bg
    });

    it('renaming Background removes isBackground and locks', () => {
      const engine = new DocumentEngine('doc-bg-test', 'BG Test', 800, 600);
      const bg = engine.addLayer('Background');
      bg.isBackground = true;
      bg.lockPosition = true;
      bg.lockRotation = true;

      engine.setLayerName(bg.id, 'My Photo');
      expect(bg.isBackground).toBeUndefined();
      expect(bg.lockPosition).toBe(false);
      expect(bg.lockRotation).toBe(false);
      expect(bg.name).toBe('My Photo');
    });

    it('snapshot preserves isBackground flag', () => {
      const engine = new DocumentEngine('doc-bg-test', 'BG Test', 800, 600);
      const bg = engine.addLayer('Background');
      bg.isBackground = true;

      const snap = engine.snapshot();
      const restoredLayer = snap.layers.find(l => l.id === bg.id);
      expect(restoredLayer?.isBackground).toBe(true);
    });

    it('flattenLayers produces a Background-flagged, locked bottom layer', () => {
      const engine = new DocumentEngine('doc-flatten-bg', 'Flatten', 800, 600);
      const bg = engine.addLayer('Background');
      bg.isBackground = true;
      bg.lockPosition = true;
      bg.lockRotation = true;
      engine.addLayer('Layer 1');
      engine.addLayer('Layer 2');

      engine.flattenLayers();

      const layers = engine.getLayers();
      expect(layers).toHaveLength(1);
      expect(layers[0].isBackground).toBe(true);
      expect(layers[0].lockPosition).toBe(true);
      expect(layers[0].lockRotation).toBe(true);
    });

    it('restore re-seats a non-bottom Background to the bottom', () => {
      const engine = new DocumentEngine('doc-restore-bg', 'Restore', 800, 600);
      const bg = engine.addLayer('Background');
      bg.isBackground = true;
      engine.addLayer('Layer 1');
      engine.addLayer('Layer 2'); // [Layer 2, Layer 1, Background]

      // Simulate a legacy / hand-edited saved file with the Background
      // at the top instead of the bottom.
      const model = engine.snapshot();
      const reordered = {
        ...model,
        layers: [model.layers[2], model.layers[0], model.layers[1]],
      };
      engine.restore(reordered);

      const layers = engine.getLayers();
      expect(layers.findIndex((l) => l.isBackground)).toBe(layers.length - 1);
    });
  });
});
