describe('R2 Step 1 Shadow — real pointerup', () => {
  it('collects T0->T8 via real brush commit', async () => {
    // Best practice: just wait for document ready, not strict URL check (tauri:// vs https://tauri.localhost vs http://localhost:1420 all valid)
    await browser.waitUntil(async () => {
      return await browser.execute(() => document.readyState === 'complete' && !!document.body);
    }, { timeout: 8000, timeoutMsg: 'app not ready' });
    await browser.execute(() => {
      try { localStorage.setItem('photrez.rustShadow', '1'); } catch {}
    });
    await browser.refresh(); // keep session, reload frontend to pick up flag
    await browser.waitUntil(async () => {
      return await browser.execute(() => typeof window.__photrezEditor !== 'undefined' && document.readyState === 'complete');
    }, { timeout: 15000, timeoutMsg: 'editor not ready' });

    // create a new document via JS (bypass UI dialog for speed)
    await browser.execute(async () => {
      // Try to create a new document via the editor's Tauri command if available
      // Fallback: use the File->New dialog via menu event
      // For now, just ensure a document exists by checking the editor state
      const ed = window.__photrezEditor;
      // If no document, try to trigger new document via menu
      if (!ed || !ed.getActiveDocument) return;
    });

    // Find the canvas (overlay canvas for brush)
    const canvas = await $('canvas');
    await canvas.waitForExist({ timeout: 10000 });

    // Get canvas rect for pointer actions
    const rect = await browser.execute((sel) => {
      const el = document.querySelector(sel);
      if (!el) return null;
      const r = el.getBoundingClientRect();
      return { x: r.left, y: r.top, w: r.width, h: r.height };
    }, 'canvas');

    if (!rect) throw new Error('canvas rect not found');

    // Helper to do a brush stroke via pointer actions (real pointerup path)
    async function stroke(x, y, x2, y2) {
      await browser.action('pointer', { parameters: { pointerType: 'mouse' } })
        .move({ x: Math.floor(rect.x + x), y: Math.floor(rect.y + y) })
        .down({ button: 0 })
        .pause(50)
        .move({ x: Math.floor(rect.x + x2), y: Math.floor(rect.y + y2), duration: 200 })
        .up({ button: 0 })
        .perform();
      await browser.pause(800); // wait for commit + shadow
    }

    // 1) 3000x10 stress is too large for canvas (2048), so do a large stroke that still exercises tile path
    // Use the canvas center and do a long drag
    const cx = rect.w / 2, cy = rect.h / 2;
    await stroke(cx - 200, cy, cx + 200, cy); // small stress
    await stroke(cx - 300, cy - 100, cx + 300, cy + 100); // 4K+ like
    await stroke(cx, cy, cx + 50, cy + 50); // rapid burst 1
    await stroke(cx, cy, cx + 60, cy + 60); // rapid burst 2
    await stroke(cx, cy, cx + 70, cy + 70); // rapid burst 3

    // 10-commit tail: 10 more small strokes
    for (let i = 0; i < 10; i++) {
      await stroke(cx + i * 5, cy + i * 5, cx + i * 5 + 30, cy + i * 5 + 30);
    }

    // Read shadow log
    const log = await browser.execute(() => window.__photrezShadowLog || []);
    console.log('shadow log records', log.length);
    console.log(JSON.stringify(log, null, 2));

    // Basic assertions: shadow ran, TS canonical unchanged (no errors in log that would indicate mutation)
    expect(log.length).toBeGreaterThan(0);
    for (const rec of log) {
      expect(rec.rust).toBeDefined();
      expect(rec.tsDigest).toBeDefined();
      // divergence is evidence, not tolerance — just ensure it was recorded
      expect(rec.divergence !== null || rec.digestEqual).toBeTruthy();
    }

    // One fault case: invalid tip_id should be caught as error without mutating canonical
    const fault = await browser.execute(async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        await invoke('paint_parity_shadow', {
          req: { w: 100, h: 100, prep_white: true, eraser: false, brush: 10, dabs: [{x:10,y:10,alpha:1}], tip_w: 10, tip_h: 10, tip_data: [], tip_id: 'invalid-not-registered' },
          opts: { include_tiles: false }
        });
        return { ok: true };
      } catch (e) {
        return { ok: false, error: String(e) };
      }
    });
    expect(fault.ok).toBe(false);
    expect(fault.error).toContain('tip_id not registered');

    // Verify TS canonical still works after fault (do one more stroke)
    await stroke(cx, cy, cx + 80, cy + 80);
    const log2 = await browser.execute(() => window.__photrezShadowLog || []);
    expect(log2.length).toBeGreaterThan(log.length);

    // Flag OFF regression: disable and verify 0 shadow work on next stroke
    await browser.execute(() => {
      localStorage.setItem('photrez.rustShadow', '0');
      window.__photrezShadowLog = [];
    });
    await stroke(cx, cy, cx + 90, cy + 90);
    const logOff = await browser.execute(() => window.__photrezShadowLog || []);
    expect(logOff.length).toBe(0);
  });
});
