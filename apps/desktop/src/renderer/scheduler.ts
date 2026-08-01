export interface FrameMetrics {
  frames: number;
  avgMs: number;
  maxMs: number;
}

export class RenderScheduler {
  private framePending = false;
  private continuousMode = false;
  private renderCallback: (() => void) | null = null;
  private rafId: number | null = null;
  private continuousRafId: number | null = null;

  // Render timing metrics since the last reset (dev-mode status-bar display).
  private frameCount = 0;
  private totalMs = 0;
  private maxMs = 0;

  constructor(renderCallback: () => void) {
    this.renderCallback = renderCallback;
  }

  requestRender(): void {
    if (this.framePending || this.continuousMode) return;
    this.framePending = true;

    this.rafId = requestAnimationFrame(() => {
      const _t0 = performance.now();
      this.framePending = false;
      this.renderCallback?.();
      const _dt = performance.now() - _t0;
      this.recordFrame(_dt);
      if (_dt > 5) console.warn(`[perf] scheduler.render: ${_dt.toFixed(1)}ms`);
    });
  }

  /**
   * Render synchronously on the calling frame. Cancels any pending deferred
   * frame so we never double-render. Use when a pan/zoom must be visible on
   * the SAME frame as a reactive overlay update (avoids a 1-frame seam
   * between the WebGL content and CSS-transformed overlays).
   */
  renderNow(): void {
    if (this.continuousMode) return; // continuous loop already renders every frame
    if (this.rafId !== null) {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
      this.framePending = false;
    }
    const _t0 = performance.now();
    this.renderCallback?.();
    const _dt = performance.now() - _t0;
    this.recordFrame(_dt);
    if (_dt > 5) console.warn(`[perf] scheduler.renderNow: ${_dt.toFixed(1)}ms`);
  }

  startContinuousRender(): void {
    if (this.continuousMode) return;
    this.continuousMode = true;

    if (this.rafId !== null) {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
      this.framePending = false;
    }

    const loop = () => {
      if (!this.continuousMode) return;
      const _t0 = performance.now();
      this.renderCallback?.();
      const _dt = performance.now() - _t0;
      this.recordFrame(_dt);
      if (_dt > 5) console.warn(`[perf] scheduler.continuousRender: ${_dt.toFixed(1)}ms`);
      this.continuousRafId = requestAnimationFrame(loop);
    };
    this.continuousRafId = requestAnimationFrame(loop);
  }

  stopContinuousRender(): void {
    if (!this.continuousMode) return;
    this.continuousMode = false;
    if (this.continuousRafId !== null) {
      cancelAnimationFrame(this.continuousRafId);
      this.continuousRafId = null;
    }
  }

  isContinuous(): boolean {
    return this.continuousMode;
  }

  /** Render timing metrics since the last reset. Zero frames → zeros. */
  getFrameMetrics(): FrameMetrics {
    return {
      frames: this.frameCount,
      avgMs: this.frameCount > 0 ? this.totalMs / this.frameCount : 0,
      maxMs: this.maxMs,
    };
  }

  /** Reset the timing window (called by the status-bar poller between samples). */
  resetFrameMetrics(): void {
    this.frameCount = 0;
    this.totalMs = 0;
    this.maxMs = 0;
  }

  private recordFrame(dt: number): void {
    this.frameCount++;
    this.totalMs += dt;
    if (dt > this.maxMs) this.maxMs = dt;
  }

  cancel(): void {
    if (this.rafId !== null) {
      cancelAnimationFrame(this.rafId);
      this.rafId = null;
    }
    this.stopContinuousRender();
    this.framePending = false;
  }

  dispose(): void {
    this.cancel();
    this.renderCallback = null;
  }
}
