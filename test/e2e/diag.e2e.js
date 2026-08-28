describe('Diagnostic — WebView state', () => {
  it('inspect once and exit', async () => {
    const state = await browser.execute(() => {
      return {
        href: location.href,
        readyState: document.readyState,
        hasBody: !!document.body,
        hasTAURI: typeof window.__TAURI__ !== 'undefined',
        hasInvoke: typeof window.__TAURI__?.core?.invoke === 'function',
        shadowLogType: typeof window.__photrezShadowLog,
        shadowFlag: (() => { try { return localStorage.getItem('photrez.rustShadow'); } catch(e){ return `err:${e.message}`; } })(),
        hasEditor: typeof window.__photrezEditor !== 'undefined',
        hasCanvas: !!document.querySelector('canvas'),
        canvasCount: document.querySelectorAll('canvas').length,
        title: document.title,
        bodyText: document.body ? document.body.innerText.slice(0,200) : null,
      };
    });
    console.log('DIAG STATE', JSON.stringify(state, null, 2));
    // Always pass — this is diagnostic, not a measurement
    expect(state.readyState).toBeDefined();
  });
});
