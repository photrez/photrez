import { EasingFn } from "./easing";

export interface CameraState {
  x: number;
  y: number;
  zoom: number;
}

export interface ImageTransformState {
  offsetX: number;
  offsetY: number;
  rotation: number;
  scale: number;
  pivotScreen: { x: number; y: number } | null;
  pivotDocument: { x: number; y: number } | null;
}

export interface AnimationState {
  from: CameraState;
  to: CameraState;
  startTime: number;
  duration: number;
  easing: EasingFn;
}

export const MIN_ZOOM = 0.01;
export const MAX_ZOOM = 100.0;

export class ViewportCamera {
  private current: CameraState = { x: 0, y: 0, zoom: 1.0 };
  private animation: AnimationState | null = null;
  private viewportWidth = 800;
  private viewportHeight = 600;
  private imageTransform: ImageTransformState = {
    offsetX: 0,
    offsetY: 0,
    rotation: 0,
    scale: 1.0,
    pivotScreen: null,
    pivotDocument: null,
  };

  public onAnimationStart?: () => void;
  public onAnimationEnd?: () => void;
  public isModernCropActive = false;

  // Pre-allocated matrix buffer — `getViewProjectionMatrix` is called every
  // render frame; allocating a Float32Array per call churns the GC. Callers
  // must consume the matrix synchronously (the renderer applies it
  // immediately), never store the reference across frames (review #26).
  private readonly matrixCache = new Float32Array(16);

  constructor(initial?: Partial<CameraState>) {
    if (initial) {
      this.current = {
        x: initial.x ?? 0,
        y: initial.y ?? 0,
        zoom: initial.zoom ?? 1.0,
      };
    }
  }

  public getState(): CameraState {
    return { ...this.current };
  }

  public setState(state: CameraState): void {
    this.current = {
      x: state.x,
      y: state.y,
      zoom: Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, state.zoom)),
    };
    if (this.animation) {
      this.animation = null;
      this.onAnimationEnd?.();
    }
  }

  public setViewportSize(w: number, h: number): void {
    this.viewportWidth = w;
    this.viewportHeight = h;
  }

  public getViewportSize(): { width: number; height: number } {
    return { width: this.viewportWidth, height: this.viewportHeight };
  }

  public setImageTransform(t: Partial<ImageTransformState>): void {
    this.imageTransform = {
      offsetX: t.offsetX ?? 0,
      offsetY: t.offsetY ?? 0,
      rotation: t.rotation ?? 0,
      scale: t.scale ?? 1.0,
      pivotScreen: t.pivotScreen ?? null,
      pivotDocument: t.pivotDocument ?? null,
    };
  }

  public getImageTransform(): ImageTransformState {
    return { ...this.imageTransform };
  }

  public resetImageTransform(): void {
    this.setImageTransform({});
  }

  public isAnimating(): boolean {
    return this.animation !== null;
  }

  public pan(dx: number, dy: number): void {
    this.current.x += dx;
    this.current.y += dy;
    if (this.animation) {
      this.animation = null;
      this.onAnimationEnd?.();
    }
  }

  public zoomToPoint(factor: number, screenX: number, screenY: number): void {
    const oldZoom = this.current.zoom;
    const newZoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, oldZoom * factor));

    // Anchor: the document position under the screen point must remain invariant.
    // docX = (screenX - camera.x) / oldZoom
    // camera.x_new = screenX - docX * newZoom
    this.current.x = screenX - ((screenX - this.current.x) / oldZoom) * newZoom;
    this.current.y = screenY - ((screenY - this.current.y) / oldZoom) * newZoom;
    this.current.zoom = newZoom;
    if (this.animation) {
      this.animation = null;
      this.onAnimationEnd?.();
    }
  }

  public animateZoomToPoint(
    factor: number,
    screenX: number,
    screenY: number,
    duration: number,
    easing: EasingFn
  ): void {
    const oldZoom = this.current.zoom;
    const newZoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, oldZoom * factor));

    const targetX = screenX - ((screenX - this.current.x) / oldZoom) * newZoom;
    const targetY = screenY - ((screenY - this.current.y) / oldZoom) * newZoom;

    this.animateTo({ x: targetX, y: targetY, zoom: newZoom }, duration, easing);
  }

  public animateTo(target: CameraState, duration: number, easing: EasingFn): void {
    this.animation = {
      from: { ...this.current },
      to: {
        x: target.x,
        y: target.y,
        zoom: Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, target.zoom)),
      },
      startTime: performance.now(),
      duration,
      easing,
    };

    if (this.onAnimationStart) {
      this.onAnimationStart();
    }
  }

  public tick(now: number): boolean {
    if (!this.animation) return false;

    const elapsed = now - this.animation.startTime;
    const t = Math.max(0, Math.min(1, elapsed / this.animation.duration));
    const eased = this.animation.easing(t);

    this.current.x = this.animation.from.x + (this.animation.to.x - this.animation.from.x) * eased;
    this.current.y = this.animation.from.y + (this.animation.to.y - this.animation.from.y) * eased;
    this.current.zoom = this.animation.from.zoom + (this.animation.to.zoom - this.animation.from.zoom) * eased;

    if (t >= 1) {
      this.current = { ...this.animation.to };
      this.animation = null;
      if (this.onAnimationEnd) {
        this.onAnimationEnd();
      }
      return false;
    }

    return true;
  }

  public getViewProjectionMatrix(canvasW?: number, canvasH?: number): Float32Array {
    // effect when canvasW/H were passed. Pure getter is the right
    // contract — callers that want to resize the camera must call
    // setViewportSize() explicitly. The optional params are still
    // accepted so existing tests that pass them continue to work
    // without forcing every call site to thread dimensions.
    const w = canvasW ?? this.viewportWidth;
    const h = canvasH ?? this.viewportHeight;

    const { x, y, zoom } = this.current;
    const m = this.matrixCache;
    m.fill(0); // both paths below write a subset of the 16 elements

    const it = this.imageTransform;
    const hasPivot = it.pivotScreen !== null && it.pivotDocument !== null;
    const zs = zoom * it.scale;

    if (!hasPivot) {
      // No pivot: offset is added to pan in screen pixel space
      m[0]  = (2 * zs) / w;
      m[5]  = (-2 * zs) / h;
      m[10] = 1;
      m[12] = -1 + ((x + it.offsetX) * 2) / w;
      m[13] =  1 + ((y + it.offsetY) * -2) / h;
      m[15] = 1;
      return m;
    }

    // With pivot: image is rendered with its pivotDocument at pivotScreen.
    // Linear part: R * (zoom*imageScale) * (p - pivotDocument)
    // Translation: +pivotScreen (in NDC, with Y flipped).
    // The camera pan is already baked into the linear part (via zoom), so
    // we do NOT add it again here — otherwise the image is double-panned
    // and pivotDocument would render at pivotScreen + pan instead of
    // pivotScreen. Similarly, offsetX/Y are ignored in pivot mode because
    // pivotScreen is the authoritative screen position of the image.
    const cosR = Math.cos((it.rotation * Math.PI) / 180);
    const sinR = Math.sin((it.rotation * Math.PI) / 180);
    const pd = it.pivotDocument!;
    const ps = it.pivotScreen!;

    m[0]  =  (2 * zs * cosR) / w;
    m[1]  = -(2 * zs * sinR) / h;
    m[4]  = -(2 * zs * sinR) / w;
    m[5]  = -(2 * zs * cosR) / h;
    m[10] = 1;
    m[12] = -1 + (2 / w) * (zs * (-cosR * pd.x + sinR * pd.y) + ps.x);
    m[13] =  1 + (2 / h) * (zs * (sinR * pd.x + cosR * pd.y) - ps.y);
    m[15] = 1;

    return m;
  }

  public screenToDocument(screenX: number, screenY: number): { x: number; y: number } {
    const { x, y, zoom } = this.current;
    return {
      x: (screenX - x) / zoom,
      y: (screenY - y) / zoom,
    };
  }

  public documentToScreen(docX: number, docY: number): { x: number; y: number } {
    const { x, y, zoom } = this.current;
    return {
      x: docX * zoom + x,
      y: docY * zoom + y,
    };
  }
}
