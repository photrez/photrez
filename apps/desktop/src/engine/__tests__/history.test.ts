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
    // No native stream is attached, so the query is not authoritative and
    // the native-authority counters report their empty defaults.
    expect(history.canUndo()).toBe(false);
    expect(history.canRedo()).toBe(false);
    expect(history.getUndoCount()).toBe(0);
    expect(history.getRedoCount()).toBe(0);
  });

  it('clear on empty history is safe', () => {
    const history = new CommandHistory();
    expect(() => history.clear()).not.toThrow();
  });

  it('max depth of zero evicts immediately (no history kept)', () => {
    const history = new CommandHistory(0);
    history.commit(createMockModel('S1'));
    expect(history.getUndoCount()).toBe(0);
    expect(history.canUndo()).toBe(false);
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

      // shared bitmap still held by m2 -> must survive eviction of m1
      expect(shared.close).not.toHaveBeenCalled();
    });

    it('does NOT close a bitmap still referenced by the live document model', () => {
      const live = { close: vi.fn() };
      const history = new CommandHistory(2);
      history.attachLiveBitmapGetter(() => [live as unknown as ImageBitmap]);
      history.commit(makeModelWithBitmap('S1', live));
      history.commit(makeModelWithBitmap('S2', { close: vi.fn() }));
      history.commit(makeModelWithBitmap('S3', { close: vi.fn() })); // evicts S1

      // live model still uses the bitmap -> must survive eviction
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
