import { describe, it, expect } from 'vitest';
import { resolveCursor } from '../viewport/cursorResolver';
import type { CursorContext } from '../viewport/cursorResolver';

describe('resolveCursor', () => {
  const base: CursorContext = {
    isSpacePressed: false,
    isPanning: false,
    activeTool: 'selection',
    isAltPressed: false,
    hoverHandle: null,
    isLayerLocked: false,
    eyedropperTarget: null,
  };

  it('returns crosshair when eyedropperTarget is set', () => {
    const ctx: CursorContext = {
      ...base,
      eyedropperTarget: '#ffffff',
    };
    expect(resolveCursor(ctx)).toBe('crosshair');
  });

  it('returns crosshair when a color picker is open (canvas pick mode)', () => {
    const ctx: CursorContext = {
      ...base,
      colorPickerOpen: true,
    };
    expect(resolveCursor(ctx)).toBe('crosshair');
  });

  it('returns grab when space is pressed', () => {
    const ctx: CursorContext = {
      ...base,
      isSpacePressed: true,
    };
    expect(resolveCursor(ctx)).toBe('grab');
  });

  it('returns grabbing when space is pressed and panning', () => {
    const ctx: CursorContext = {
      ...base,
      isSpacePressed: true,
      isPanning: true,
    };
    expect(resolveCursor(ctx)).toBe('grabbing');
  });

  it('returns copy when alt is pressed and activeTool is brush', () => {
    const ctx: CursorContext = {
      ...base,
      activeTool: 'brush',
      isAltPressed: true,
    };
    expect(resolveCursor(ctx)).toBe('copy');
  });

  it('returns copy when alt is pressed and activeTool is eraser', () => {
    const ctx: CursorContext = {
      ...base,
      activeTool: 'eraser',
      isAltPressed: true,
    };
    expect(resolveCursor(ctx)).toBe('copy');
  });

  describe('move tool with handles', () => {
    it('returns default when layer is locked', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        isLayerLocked: true,
        hoverHandle: 'move',
      };
      expect(resolveCursor(ctx)).toBe('default');
    });

    it('returns move when hoverHandle is move', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        hoverHandle: 'move',
      };
      expect(resolveCursor(ctx)).toBe('move');
    });

    it('returns nwse-resize for nw handle', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        hoverHandle: 'nw',
        layerRotation: 0,
        layerScaleX: 1,
        layerScaleY: 1,
      };
      expect(resolveCursor(ctx)).toBe('nwse-resize');
    });

    it('returns nwse-resize for se handle (45° base angle)', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        hoverHandle: 'se',
        layerRotation: 0,
        layerScaleX: 1,
        layerScaleY: 1,
      };
      expect(resolveCursor(ctx)).toBe('nwse-resize');
    });

    it('returns ns-resize for n handle', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        hoverHandle: 'n',
        layerRotation: 0,
        layerScaleX: 1,
        layerScaleY: 1,
      };
      expect(resolveCursor(ctx)).toBe('ns-resize');
    });

    it('returns ew-resize for e handle', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        hoverHandle: 'e',
        layerRotation: 0,
        layerScaleX: 1,
        layerScaleY: 1,
      };
      expect(resolveCursor(ctx)).toBe('ew-resize');
    });

    it('returns default when no hoverHandle', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        hoverHandle: null,
      };
      expect(resolveCursor(ctx)).toBe('default');
    });
  });

  it('returns crosshair for selection tool', () => {
    const ctx: CursorContext = {
      ...base,
      activeTool: 'selection',
    };
    expect(resolveCursor(ctx)).toBe('crosshair');
  });

  it('returns crosshair for crop tool when not over a handle', () => {
    const ctx: CursorContext = {
      ...base,
      activeTool: 'crop',
    };
    expect(resolveCursor(ctx)).toBe('crosshair');
  });

  it('returns resize cursor for crop tool when hovering a handle', () => {
    const ctx: CursorContext = {
      ...base,
      activeTool: 'crop',
      hoverHandle: 'se',
    };
    expect(resolveCursor(ctx)).toBe('nwse-resize');
  });

  it('returns move cursor for crop tool when hovering inside crop box', () => {
    const ctx: CursorContext = {
      ...base,
      activeTool: 'crop',
      hoverHandle: 'move',
    };
    expect(resolveCursor(ctx)).toBe('move');
  });

  it('returns none for brush tool', () => {
    const ctx: CursorContext = {
      ...base,
      activeTool: 'brush',
    };
    expect(resolveCursor(ctx)).toBe('none');
  });

  it('returns none for eraser tool', () => {
    const ctx: CursorContext = {
      ...base,
      activeTool: 'eraser',
    };
    expect(resolveCursor(ctx)).toBe('none');
  });

  it('returns crosshair for eyedropper tool', () => {
    const ctx: CursorContext = {
      ...base,
      activeTool: 'eyedropper',
    };
    expect(resolveCursor(ctx)).toBe('crosshair');
  });

  it('returns default for unknown tool', () => {
    const ctx: CursorContext = {
      ...base,
      activeTool: 'unknown' as unknown as import("../components/editor/tools/toolTypes").ToolId,
    };
    expect(resolveCursor(ctx)).toBe('default');
  });

  it('returns text (I-beam) cursor for the text tool', () => {
    const ctx: CursorContext = {
      ...base,
      activeTool: 'text',
    };
    expect(resolveCursor(ctx)).toBe('text');
  });

  describe('rotate handle cursor', () => {
    it('returns SVG rotate cursor for rotate handle with hoverPos', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        hoverHandle: 'rotate',
        hoverPos: { x: 300, y: 150 },
        layerBoundingBox: { x: 100, y: 100, w: 200, h: 100 },
        layerRotation: 0,
        layerScaleX: 1,
        layerScaleY: 1,
      };
      const cursor = resolveCursor(ctx);
      expect(cursor).toContain('data:image/svg+xml');
    });

    it('returns static rotate cursor for rotate handle without hoverPos', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        hoverHandle: 'rotate',
        hoverPos: null,
        layerBoundingBox: { x: 100, y: 100, w: 200, h: 100 },
        layerRotation: 0,
        layerScaleX: 1,
        layerScaleY: 1,
      };
      const cursor = resolveCursor(ctx);
      expect(cursor).toContain('data:image/svg+xml');
    });
  });

  describe('rotate handle cursor with corner prefix', () => {
    it('returns rotate cursor for rotate-nw with hoverPos', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        hoverHandle: 'rotate-nw',
        hoverPos: { x: 300, y: 150 },
        layerBoundingBox: { x: 100, y: 100, w: 200, h: 100 },
        layerRotation: 0,
        layerScaleX: 1,
        layerScaleY: 1,
      };
      const cursor = resolveCursor(ctx);
      expect(cursor).toContain('data:image/svg+xml');
    });

    it('does NOT return resize cursor for rotate-nw', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        hoverHandle: 'rotate-nw',
        hoverPos: null,
        layerBoundingBox: { x: 100, y: 100, w: 200, h: 100 },
        layerRotation: 0,
        layerScaleX: 1,
        layerScaleY: 1,
      };
      const cursor = resolveCursor(ctx);
      // Should NOT be a resize cursor
      expect(cursor).not.toMatch(/^(nw|n|ne|e|se|s|sw|w)-resize$/);
      // Should be the rotate SVG cursor
      expect(cursor).toContain('data:image/svg+xml');
    });

    it('returns rotate cursor for rotate-se with hoverPos', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        hoverHandle: 'rotate-se',
        hoverPos: { x: 400, y: 300 },
        layerBoundingBox: { x: 100, y: 100, w: 200, h: 100 },
        layerRotation: 0,
        layerScaleX: 1,
        layerScaleY: 1,
      };
      const cursor = resolveCursor(ctx);
      expect(cursor).toContain('data:image/svg+xml');
    });
  });

  describe('resize handle cursors', () => {
    it('returns ew-resize for e handle with no rotation', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        hoverHandle: 'e',
        layerRotation: 0,
        layerScaleX: 1,
        layerScaleY: 1,
      };
      expect(resolveCursor(ctx)).toBe('ew-resize');
    });

    it('returns ns-resize for n handle with no rotation', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        hoverHandle: 'n',
        layerRotation: 0,
        layerScaleX: 1,
        layerScaleY: 1,
      };
      expect(resolveCursor(ctx)).toBe('ns-resize');
    });

    it('returns nwse-resize for nw handle with negative scaleX', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'move',
        hoverHandle: 'nw',
        layerRotation: 0,
        layerScaleX: -1,
        layerScaleY: 1,
      };
      expect(resolveCursor(ctx)).toBe('nwse-resize');
    });
  });

  describe('tool switching clears hover', () => {
    it('returns default when switching from move to brush', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'brush',
        hoverHandle: 'move',
      };
      expect(resolveCursor(ctx)).toBe('none');
    });

    it('returns default when switching from move to selection', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'selection',
        hoverHandle: 'move',
      };
      expect(resolveCursor(ctx)).toBe('crosshair');
    });

    it('returns SVG cursor for paintBucket tool', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'paintBucket' as any,
      };
      const cursor = resolveCursor(ctx);
      expect(cursor).toContain('data:image/svg+xml');
    });

    it('returns SVG cursor for gradient tool', () => {
      const ctx: CursorContext = {
        ...base,
        activeTool: 'gradient' as any,
      };
      const cursor = resolveCursor(ctx);
      expect(cursor).toContain('data:image/svg+xml');
    });
  });
});
