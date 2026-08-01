import { describe, it, expect, vi } from 'vitest';
import { CommandHistory } from '../history';
import type { DocumentModel } from '../types';

const createMockModel = (name: string, overrides: Partial<DocumentModel> = {}): DocumentModel => ({
  id: 'doc-1',
  name,
  width: 800,
  height: 600,
  layers: [],
  activeLayerId: null,
  selection: null,
  viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
  dirty: false,
  ...overrides,
});
describe('CommandHistory', () => {
  it('starts with empty stacks', () => {
    const history = new CommandHistory();
    expect(history.canUndo()).toBe(false);
    expect(history.canRedo()).toBe(false);
    expect(history.getUndoCount()).toBe(0);
    expect(history.getRedoCount()).toBe(0);
  });

  it('commit pushes to undo stack and clears redo stack', () => {
    const history = new CommandHistory();
    history.commit(createMockModel('State 1'));
    history.commit(createMockModel('State 2'));

    expect(history.canUndo()).toBe(true);
    expect(history.getUndoCount()).toBe(2);

    history.undo(createMockModel('State 3'));
    expect(history.canRedo()).toBe(true);

    history.commit(createMockModel('State 4'));
    expect(history.canRedo()).toBe(false);
    expect(history.getRedoCount()).toBe(0);
  });

  it('performs undo and redo round-trips correctly', () => {
    const history = new CommandHistory();
    const state1 = createMockModel('State 1');
    const state2 = createMockModel('State 2');
    const state3 = createMockModel('State 3');

    history.commit(state1);
    history.commit(state2);

    const undone = history.undo(state3);
    expect(undone?.name).toBe('State 2');
    expect(history.getRedoCount()).toBe(1);

    const undone2 = history.undo(state2);
    expect(undone2?.name).toBe('State 1');
    expect(history.getRedoCount()).toBe(2);

    const redone = history.redo(state1);
    expect(redone?.name).toBe('State 2');
  });

  it('undo returns null when stack is empty', () => {
    const history = new CommandHistory();
    expect(history.undo(createMockModel('current'))).toBeNull();
  });

  it('redo returns null when stack is empty', () => {
    const history = new CommandHistory();
    expect(history.redo(createMockModel('current'))).toBeNull();
  });

  it('clear resets both undo and redo stacks', () => {
    const history = new CommandHistory();
    history.commit(createMockModel('S1'));
    history.commit(createMockModel('S2'));
    history.undo(createMockModel('S3'));

    expect(history.canUndo()).toBe(true);
    expect(history.canRedo()).toBe(true);

    history.clear();

    expect(history.canUndo()).toBe(false);
    expect(history.canRedo()).toBe(false);
    expect(history.getUndoCount()).toBe(0);
    expect(history.getRedoCount()).toBe(0);
  });

  it('clear on empty history is safe', () => {
    const history = new CommandHistory();
    expect(() => history.clear()).not.toThrow();
  });

  it('respects max depth and evicts oldest', () => {
    const history = new CommandHistory(3);
    history.commit(createMockModel('S1'));
    history.commit(createMockModel('S2'));
    history.commit(createMockModel('S3'));
    history.commit(createMockModel('S4'));

    expect(history.getUndoCount()).toBe(3);

    const u1 = history.undo(createMockModel('S5'));
    expect(u1?.name).toBe('S4');
    const u2 = history.undo(u1!);
    expect(u2?.name).toBe('S3');
    const u3 = history.undo(u2!);
    expect(u3?.name).toBe('S2');
    const u4 = history.undo(u3!);
    expect(u4).toBeNull();
  });

  it('does not mutate the committed snapshot on undo/redo', () => {
    const history = new CommandHistory();
    const original = createMockModel('Original');
    history.commit(original);

    const current = createMockModel('Current');
    current.name = 'Mutated';

    const restored = history.undo(current);
    expect(original.name).toBe('Original');
    expect(restored?.name).toBe('Original');
  });

  it('undo saves current state to redo without mutation', () => {
    const history = new CommandHistory();
    history.commit(createMockModel('Original'));

    const current = createMockModel('After Edit');
    history.undo(current);

    expect(history.getRedoCount()).toBe(1);
  });

  it('max depth of zero evicts immediately (no history kept)', () => {
    const history = new CommandHistory(0);
    history.commit(createMockModel('S1'));
    expect(history.getUndoCount()).toBe(0);
    expect(history.canUndo()).toBe(false);
  });

  it('undo and redo with complex state preserves all fields', () => {
    const history = new CommandHistory();
    const complex = createMockModel('Complex', {
      width: 1920,
      height: 1080,
      activeLayerId: 'layer-42',
      selection: { x: 10, y: 20, width: 100, height: 200, angle: 0 },
      viewport: { panX: 50, panY: -30, zoom: 1.5, rotation: 0 },
      dirty: true,
      layers: [
        {
          id: 'layer-1', name: 'BG', type: 'raster', visible: true,
          opacity: 0.8, locked: false, lockTransparency: false,
          lockPosition: false, lockRotation: false,
          blendMode: 'normal',
          transform: { x: 10, y: 20, scaleX: 2, scaleY: 1.5, rotation: 45, flipH: false, flipV: false },
          width: 400, height: 300,
          imageBitmap: null,
        }
      ],
    });
    history.commit(complex);

    const current = createMockModel('Current');
    const restored = history.undo(current);

    expect(restored?.width).toBe(1920);
    expect(restored?.height).toBe(1080);
    expect(restored?.activeLayerId).toBe('layer-42');
    expect(restored?.selection).toEqual({ x: 10, y: 20, width: 100, height: 200, angle: 0 });
    expect(restored?.viewport).toEqual({ panX: 50, panY: -30, zoom: 1.5, rotation: 0 });
    expect(restored?.dirty).toBe(true);
    expect(restored?.layers.length).toBe(1);

    const layer = restored!.layers[0];
    expect(layer.name).toBe('BG');
    expect(layer.transform).toEqual({ x: 10, y: 20, scaleX: 2, scaleY: 1.5, rotation: 45, flipH: false, flipV: false });
    expect(layer.opacity).toBe(0.8);
    expect(layer.blendMode).toBe('normal');
  });

  describe('labeled operations and history stack list', () => {
    it('saves operation labels and maps them to getHistoryStack correctly', () => {
      const history = new CommandHistory();
      const state1 = createMockModel('State 1');
      const state2 = createMockModel('State 2');
      const state3 = createMockModel('State 3');

      // Initial stack has just Open
      expect(history.getHistoryStack()).toEqual([
        { label: 'Open', isRedo: false }
      ]);

      // Commit 1 with custom label
      history.commit(state1, 'Add Layer');
      expect(history.getHistoryStack()).toEqual([
        { label: 'Open', isRedo: false },
        { label: 'Add Layer', isRedo: false }
      ]);

      // Commit 2 with default fallback
      history.commit(state2);
      expect(history.getHistoryStack()).toEqual([
        { label: 'Open', isRedo: false },
        { label: 'Add Layer', isRedo: false },
        { label: 'Unknown Operation', isRedo: false }
      ]);

      // Undo once -> last action becomes Redo (future state)
      history.undo(state3);
      expect(history.getHistoryStack()).toEqual([
        { label: 'Open', isRedo: false },
        { label: 'Add Layer', isRedo: false },
        { label: 'Unknown Operation', isRedo: true }
      ]);

      // Undo twice
      history.undo(state2);
      expect(history.getHistoryStack()).toEqual([
        { label: 'Open', isRedo: false },
        { label: 'Add Layer', isRedo: true },
        { label: 'Unknown Operation', isRedo: true }
      ]);

      // Redo once
      history.redo(state1);
      expect(history.getHistoryStack()).toEqual([
        { label: 'Open', isRedo: false },
        { label: 'Add Layer', isRedo: false },
        { label: 'Unknown Operation', isRedo: true }
      ]);
    });
  });

  describe('VRAM bitmap disposal on eviction', () => {
    const makeModelWithBitmap = (name: string, bitmap: { close: () => void }) => {
      const b = bitmap as unknown as ImageBitmap;
      return createMockModel(name, {
        layers: [
          { id: 'l1', name: 'L1', visible: true, opacity: 1, width: 10, height: 10, transform: { x: 0, y: 0 }, imageBitmap: b } as never,
        ],
      });
    };

    it('closes the evicted snapshot bitmap when it is no longer referenced', () => {
      const close = vi.fn();
      const history = new CommandHistory(2);
      history.commit(makeModelWithBitmap('S1', { close }));
      history.commit(makeModelWithBitmap('S2', { close: vi.fn() }));
      // S3 pushes S1 out (depth 2)
      history.commit(makeModelWithBitmap('S3', { close: vi.fn() }));

      expect(close).toHaveBeenCalledTimes(1);
    });

    it('does NOT close a bitmap still referenced by another snapshot', () => {
      const shared = { close: vi.fn() };
      const history = new CommandHistory(2);
      const m1 = makeModelWithBitmap('S1', shared);
      const m2 = makeModelWithBitmap('S2', shared); // same bitmap reference
      history.commit(m1);
      history.commit(m2);
      history.commit(makeModelWithBitmap('S3', { close: vi.fn() })); // evicts m1

      // shared bitmap still held by m2 → must survive eviction of m1
      expect(shared.close).not.toHaveBeenCalled();
    });

    it('does NOT close a bitmap still referenced by the live document model', () => {
      const live = { close: vi.fn() };
      const history = new CommandHistory(2);
      history.attachLiveBitmapGetter(() => [live as unknown as ImageBitmap]);
      history.commit(makeModelWithBitmap('S1', live));
      history.commit(makeModelWithBitmap('S2', { close: vi.fn() }));
      history.commit(makeModelWithBitmap('S3', { close: vi.fn() })); // evicts S1

      // live model still uses the bitmap → must survive eviction
      expect(live.close).not.toHaveBeenCalled();
    });

    it('closes the bitmap once the live model no longer references it', () => {
      const gone = { close: vi.fn() };
      const history = new CommandHistory(2);
      history.attachLiveBitmapGetter(() => []); // live model no longer holds it
      history.commit(makeModelWithBitmap('S1', gone));
      history.commit(makeModelWithBitmap('S2', { close: vi.fn() }));
      history.commit(makeModelWithBitmap('S3', { close: vi.fn() })); // evicts S1

      expect(gone.close).toHaveBeenCalledTimes(1);
    });

    it('clear() disposes all stacked bitmaps (stacks are dropped)', () => {
      const shared = { close: vi.fn() };
      const other = { close: vi.fn() };
      const history = new CommandHistory();
      history.commit(makeModelWithBitmap('S1', shared));
      history.commit(makeModelWithBitmap('S2', other));
      history.clear();

      // Both stacks are emptied by clear(), so every bitmap they held is
      // unreferenced and must be closed.
      expect(shared.close).toHaveBeenCalledTimes(1);
      expect(other.close).toHaveBeenCalledTimes(1);
    });
  });
});
