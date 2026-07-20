import type { RenderBackend, RenderCapabilities, TextureRef } from "./types";
import type { RenderState, BlendMode } from "../engine/types";
import type { BasicAdjustment } from "../engine/layerAdjustments";
import { setDeviceMaxTextureSize } from "../engine/types";
import {
  VERTEX_SHADER_SOURCE,
  FRAGMENT_SHADER_SOURCE,
  CHECKERBOARD_FRAGMENT_SOURCE,
  CHECKERBOARD_VERTEX_SOURCE,
} from "./shaders";
import { getCheckerboardColors } from "./checkerboard";
import { blendModeToShaderId } from "../engine/blendModes";

export function projectDocumentScissor(
  viewProj: Float32Array,
  docW: number,
  docH: number,
  canvasW: number,
  canvasH: number,
): { x: number; y: number; width: number; height: number } {
  const project = (x: number, y: number) => {
    const ndcX = viewProj[0] * x + viewProj[4] * y + viewProj[12];
    const ndcY = viewProj[1] * x + viewProj[5] * y + viewProj[13];
    return {
      x: ((ndcX + 1) / 2) * canvasW,
      y: ((1 - ndcY) / 2) * canvasH,
    };
  };

  const corners = [
    project(0, 0),
    project(docW, 0),
    project(0, docH),
    project(docW, docH),
  ];
  const minX = Math.max(0, Math.floor(Math.min(...corners.map((p) => p.x))));
  const maxX = Math.min(canvasW, Math.ceil(Math.max(...corners.map((p) => p.x))));
  const minYTop = Math.max(0, Math.floor(Math.min(...corners.map((p) => p.y))));
  const maxYTop = Math.min(canvasH, Math.ceil(Math.max(...corners.map((p) => p.y))));
  const width = Math.max(0, maxX - minX);
  const height = Math.max(0, maxYTop - minYTop);

  return {
    x: minX,
    y: Math.max(0, canvasH - maxYTop),
    width,
    height,
  };
}

/**
 * Computes the uniform values for the inter-layer ping-pong COPY pass.
 *
 * Regression note (2026-06-18): the copy must cover the ENTIRE FBO
 * (logical viewport), not just the doc-coord region. The sampler reads the
 * whole source FBO via texCoord 0..1; if the destination quad only writes
 * the doc-region, the source FBO (layer + transparent margins) is squeezed
 * into that smaller region — previous layers visually shrank by the
 * doc/viewport ratio on every layer above them. Merging masked the bug
 * because a single layer skips the copy branch entirely.
 */
export function getInterLayerCopyQuad(
  logicalWidth: number,
  logicalHeight: number,
): { rect: [number, number, number, number]; center: [number, number] } {
  return {
    rect: [0, 0, logicalWidth, logicalHeight],
    center: [logicalWidth / 2, logicalHeight / 2],
  };
}

export function getRequiredUniformLocation(
  gl: WebGL2RenderingContext,
  program: WebGLProgram,
  name: string,
): WebGLUniformLocation {
  const location = gl.getUniformLocation(program, name);
  if (!location) {
    throw new Error(`Required WebGL uniform not found: ${name}`);
  }
  return location;
}

export const WEBGL2_CONTEXT_OPTIONS: WebGLContextAttributes = {
  premultipliedAlpha: false,
  alpha: true,
  antialias: true,
  preserveDrawingBuffer: false,
};

export const WEBGL2_CONTEXT_RESTORED_EVENT = "photrez:webglcontextrestored";

export class WebGL2Backend implements RenderBackend {
  readonly name = "webgl2";
  readonly capabilities: RenderCapabilities;

  private gl: WebGL2RenderingContext | null = null;
  private canvas: HTMLCanvasElement | null = null;

  // Shaders
  private layerProgram: WebGLProgram | null = null;
  private checkerboardProgram: WebGLProgram | null = null;

  // Uniform locations
  private layerUniforms: {
    viewProj: WebGLUniformLocation;
    layerRect: WebGLUniformLocation;
    layerCenter: WebGLUniformLocation;
    layerRotation: WebGLUniformLocation;
    flipSign: WebGLUniformLocation;
    texture: WebGLUniformLocation;
    backdrop: WebGLUniformLocation;
    opacity: WebGLUniformLocation;
    blendMode: WebGLUniformLocation;
    useBackdrop: WebGLUniformLocation;
    flipTexY: WebGLUniformLocation;
    outputStraight: WebGLUniformLocation;
    resolution: WebGLUniformLocation;
    adjustment: WebGLUniformLocation;
  } | null = null;

  private checkerboardUniforms: {
    resolution: WebGLUniformLocation | null;
    checkSize: WebGLUniformLocation | null;
    color1: WebGLUniformLocation | null;
    color2: WebGLUniformLocation | null;
  } | null = null;

  // VAO (empty - we use gl_VertexID)
  private vao: WebGLVertexArrayObject | null = null;

  // Textures map
  private textures: Map<string, TextureRef> = new Map();
  private magFilterNearest = false;

  // Ping-pong Framebuffers & Textures
  private pingPongFbos: [WebGLFramebuffer | null, WebGLFramebuffer | null] = [null, null];
  private pingPongTextures: [WebGLTexture | null, WebGLTexture | null] = [null, null];
  private currentWidth = 0;
  private currentHeight = 0;
  private logicalWidth = 800;
  private logicalHeight = 600;
  private contextLost = false;

  constructor() {
    this.capabilities = {
      backend: "webgl2",
      maxTextureSize: 2048, // Fallback, updated during initialize
      supportsFloatTextures: false,
      supportsLinearFilteringFloat: false
    };
  }

  initialize(canvas: HTMLCanvasElement): void {
    if (this.canvas && this.canvas !== canvas) {
      this.removeContextListeners(this.canvas);
    }

    this.canvas = canvas;
    this.gl = canvas.getContext("webgl2", WEBGL2_CONTEXT_OPTIONS);

    if (!this.gl) {
      throw new Error("WebGL2 not supported on this platform/browser");
    }

    this.contextLost = false;
    canvas.addEventListener("webglcontextlost", this.handleContextLost);
    canvas.addEventListener("webglcontextrestored", this.handleContextRestored);
    this.initializeGpuResources();
  }

  private initializeGpuResources(): void {
    const gl = this.gl;
    if (!gl || this.contextLost || gl.isContextLost()) return;

    // Compile programs
    const vs = this.compileShader(gl.VERTEX_SHADER, VERTEX_SHADER_SOURCE);
    const fs = this.compileShader(gl.FRAGMENT_SHADER, FRAGMENT_SHADER_SOURCE);
    this.layerProgram = this.createProgram(vs, fs);

    const checkFs = this.compileShader(gl.FRAGMENT_SHADER, CHECKERBOARD_FRAGMENT_SOURCE);
    const checkVs = this.compileShader(gl.VERTEX_SHADER, CHECKERBOARD_VERTEX_SOURCE);
    this.checkerboardProgram = this.createProgram(checkVs, checkFs);

    // Get Uniforms
    this.layerUniforms = {
      viewProj: getRequiredUniformLocation(gl, this.layerProgram, "u_viewProj"),
      layerRect: getRequiredUniformLocation(gl, this.layerProgram, "u_layerRect"),
      layerCenter: getRequiredUniformLocation(gl, this.layerProgram, "u_layerCenter"),
      layerRotation: getRequiredUniformLocation(gl, this.layerProgram, "u_layerRotation"),
      flipSign: getRequiredUniformLocation(gl, this.layerProgram, "u_flipSign"),
      texture: getRequiredUniformLocation(gl, this.layerProgram, "u_texture"),
      backdrop: getRequiredUniformLocation(gl, this.layerProgram, "u_backdrop"),
      opacity: getRequiredUniformLocation(gl, this.layerProgram, "u_opacity"),
      blendMode: getRequiredUniformLocation(gl, this.layerProgram, "u_blendMode"),
      useBackdrop: getRequiredUniformLocation(gl, this.layerProgram, "u_useBackdrop"),
      flipTexY: getRequiredUniformLocation(gl, this.layerProgram, "u_flipTexY"),
      outputStraight: getRequiredUniformLocation(gl, this.layerProgram, "u_outputStraight"),
      resolution: getRequiredUniformLocation(gl, this.layerProgram, "u_resolution"),
      adjustment: getRequiredUniformLocation(gl, this.layerProgram, "u_adjustment")
    };

    this.checkerboardUniforms = {
      resolution: gl.getUniformLocation(this.checkerboardProgram, "u_resolution"),
      checkSize: gl.getUniformLocation(this.checkerboardProgram, "u_checkSize"),
      color1: gl.getUniformLocation(this.checkerboardProgram, "u_color1"),
      color2: gl.getUniformLocation(this.checkerboardProgram, "u_color2"),
    };

    // Setup empty VAO
    this.vao = gl.createVertexArray();

    // Enable Blend — premultiplied alpha so transparent edges don't darken.
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);

    // Update capabilities
    this.capabilities.maxTextureSize = gl.getParameter(gl.MAX_TEXTURE_SIZE);
    setDeviceMaxTextureSize(this.capabilities.maxTextureSize);
  }

  private handleContextLost = (event: Event): void => {
    event.preventDefault();
    this.contextLost = true;
    this.clearGpuResourceRefs();
  };

  private handleContextRestored = (): void => {
    this.contextLost = false;
    this.clearGpuResourceRefs();
    this.initializeGpuResources();
    this.canvas?.dispatchEvent(new CustomEvent(WEBGL2_CONTEXT_RESTORED_EVENT));
  };

  private removeContextListeners(canvas: HTMLCanvasElement): void {
    canvas.removeEventListener("webglcontextlost", this.handleContextLost);
    canvas.removeEventListener("webglcontextrestored", this.handleContextRestored);
  }

  private clearGpuResourceRefs(): void {
    this.layerProgram = null;
    this.checkerboardProgram = null;
    this.layerUniforms = null;
    this.checkerboardUniforms = null;
    this.vao = null;
    this.textures.clear();
    this.pingPongFbos = [null, null];
    this.pingPongTextures = [null, null];
    this.currentWidth = 0;
    this.currentHeight = 0;
  }

  uploadImage(layerId: string, source: ImageBitmap, dirtyRect?: { x: number; y: number; width: number; height: number }): TextureRef {
    const gl = this.gl;
    if (!gl) throw new Error("Renderer not initialized");
    if (this.contextLost || gl.isContextLost()) {
      throw new Error("Renderer context is lost; upload is paused until restore");
    }

    const existing = this.textures.get(layerId);
    if (
      dirtyRect &&
      existing &&
      existing.width === source.width &&
      existing.height === source.height &&
      dirtyRect.width > 0 &&
      dirtyRect.height > 0
    ) {
      // Patch only the changed sub-rect — avoids re-uploading the full layer texture.
      const { x, y, width, height } = dirtyRect;
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const ctx = canvas.getContext("2d");
      if (ctx) {
        ctx.drawImage(source, x, y, width, height, 0, 0, width, height);
        gl.bindTexture(gl.TEXTURE_2D, existing.texture);
        gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, true);
        gl.texSubImage2D(gl.TEXTURE_2D, 0, x, y, gl.RGBA, gl.UNSIGNED_BYTE, canvas);
        return existing;
      }
      // ctx unavailable (e.g. test env) → fall through to full upload below.
    }

    this.destroyTexture(layerId); // delete existing

    const texture = gl.createTexture();
    if (!texture) throw new Error("Failed to create WebGL2 texture");

    gl.bindTexture(gl.TEXTURE_2D, texture);

    // Upload premultiplied so LINEAR filtering at transparency edges blends in
    // premultiplied space (no black pull-in from (0,0,0,0) neighbors).
    gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, true);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, source);

    // Filtering: LINEAR by default; render() switches MAG to NEAREST above
    // 200% zoom for crisp pixel-level editing (see magFilterNearest).
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, this.magFilterNearest ? gl.NEAREST : gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

    const ref: TextureRef = {
      id: `tex-${layerId}`,
      texture,
      width: source.width,
      height: source.height
    };

    this.textures.set(layerId, ref);
    return ref;
  }

  destroyTexture(layerId: string): void {
    const ref = this.textures.get(layerId);
    if (ref && this.gl && !this.contextLost && !this.gl.isContextLost()) {
      this.gl.deleteTexture(ref.texture);
    }
    this.textures.delete(layerId);
  }

  /**
   * Bake a layer's basic adjustment into pixels via the GPU and return an
   * ImageBitmap, or null when baking is unavailable (no context / test env).
   *
   * Renders the layer through the same adjustment shader used for live
   * preview (u_adjustment) into a layer-sized FBO, then reads the pixels
   * back. The layer texture is uploaded premultiplied, so the FBO readback is
   * premultiplied + bottom-left origin; we flip rows and un-premultiply to
   * match `bakeAdjustmentToBitmap` (top-left, straight-alpha) so the result
   * can replace the layer's stored bitmap and re-upload cleanly.
   */
  bakeLayerToBitmap(
    layerId: string,
    width: number,
    height: number,
    adjustment: BasicAdjustment,
  ): ImageBitmap | null {
    const gl = this.gl;
    if (!gl || !this.layerProgram || !this.layerUniforms) return null;
    if (this.contextLost || gl.isContextLost()) return null;
    const ref = this.textures.get(layerId);
    if (!ref || !ref.texture) return null;

    // Allocate a temporary layer-sized FBO + color texture.
    const texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    const fbo = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);

    const prevBlend = gl.isEnabled(gl.BLEND);
    gl.disable(gl.BLEND);

    // Render the layer through the adjustment shader into the FBO. Sample the
    // raw layer texture but flip Y so the readback is already top-down, and
    // request straight-alpha output so no JS post-pass is needed.
    gl.viewport(0, 0, width, height);
    gl.useProgram(this.layerProgram);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, ref.texture);
    gl.uniform1i(this.layerUniforms.texture, 0);

    const viewProj = this.computeViewMatrix(width, height);
    gl.uniformMatrix4fv(this.layerUniforms.viewProj, false, viewProj);
    gl.uniform1f(this.layerUniforms.opacity, 1.0);
    gl.uniform1i(this.layerUniforms.blendMode, 0); // Normal
    gl.uniform1i(this.layerUniforms.useBackdrop, 0);
    gl.uniform1i(this.layerUniforms.flipTexY, 1); // flip Y so readback is top-down
    gl.uniform1i(this.layerUniforms.outputStraight, 1); // straight-alpha output
    gl.uniform3f(
      this.layerUniforms.adjustment,
      adjustment.brightness,
      adjustment.contrast,
      adjustment.saturation,
    );
    gl.uniform4f(this.layerUniforms.layerRect, 0, 0, width, height);
    gl.uniform2f(this.layerUniforms.layerCenter, width / 2, height / 2);
    gl.uniform1f(this.layerUniforms.layerRotation, 0);
    gl.uniform2f(this.layerUniforms.flipSign, 1, 1);

    gl.drawArrays(gl.TRIANGLES, 0, 6);

    const buf = new Uint8ClampedArray(width * height * 4);
    gl.readPixels(0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, buf);

    // Restore GL state.
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.bindTexture(gl.TEXTURE_2D, null);
    gl.deleteFramebuffer(fbo);
    gl.deleteTexture(texture);
    if (prevBlend) gl.enable(gl.BLEND);

    // buf is already top-down straight-alpha: flipTexY flips the sample and
    // u_outputStraight un-premultiplies in-shader, so no JS post-pass is needed.
    try {
      let scratchCanvas: OffscreenCanvas | HTMLCanvasElement;
      try {
        scratchCanvas = new OffscreenCanvas(width, height);
      } catch {
        scratchCanvas = document.createElement("canvas");
        scratchCanvas.width = width;
        scratchCanvas.height = height;
      }
      const ctx = scratchCanvas.getContext("2d") as
        | OffscreenCanvasRenderingContext2D
        | CanvasRenderingContext2D
        | null;
      if (!ctx) return null;
      ctx.putImageData(new ImageData(buf, width, height),0,0);
      return (scratchCanvas as OffscreenCanvas).transferToImageBitmap();
    } catch {
      return null;
    }
  }

  render(state: RenderState, viewProjectionMatrix?: Float32Array): void {
    const _t0 = performance.now();
    const gl = this.gl;
    const canvas = this.canvas;
    if (!gl || !canvas) return;
    if (this.contextLost || gl.isContextLost()) return;

    // Crisp pixel editing above 200% zoom: switch texture MAG filter to
    // NEAREST so magnified pixels are sharp instead of bilinear-blurred.
    // Only touch the bound textures when the mode actually changes.
    const wantNearest = state.viewport.zoom > 2;
    if (wantNearest !== this.magFilterNearest) {
      this.magFilterNearest = wantNearest;
      for (const ref of this.textures.values()) {
        gl.bindTexture(gl.TEXTURE_2D, ref.texture);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, wantNearest ? gl.NEAREST : gl.LINEAR);
      }
    }

    // Ensure ping-pong FBOs exist and are sized properly
    const docW = state.documentSize.width;
    const docH = state.documentSize.height;
    const viewProj = viewProjectionMatrix || this.computeViewMatrix(docW, docH);

    // Filter visible layers with textures
    const visibleLayers = [];
    for (let i = state.layers.length - 1; i >= 0; i--) {
      const renderLayer = state.layers[i];
      if (renderLayer.visible && this.textures.has(renderLayer.id)) {
        visibleLayers.push(renderLayer);
      }
    }

    // Clear stale texture unit bindings from previous frame.
    // TEXTURE1 can retain a reference to the previous frame's pingPong texture.
    // If the current FBO's color attachment is the same texture, WebGL detects
    // a feedback loop and silently drops the draw — even if the shader never
    // reads from that sampler (the driver checks at draw time, not per-branch).
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, null);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, null);
    gl.activeTexture(gl.TEXTURE0);

    let activeFboIndex = 0;

    if (visibleLayers.length > 0 && this.layerProgram && this.layerUniforms) {
      // Disable BLEND during FBO compositing — the shader does its own
      // src-over-dst compositing. GL_BLEND would double-blend and corrupt
      // any pixel with alpha < 1.0 (premultiplied-alpha corruption).
      gl.disable(gl.BLEND);

      gl.useProgram(this.layerProgram);
      gl.bindVertexArray(this.vao);
      
      // Clear both FBOs
      for (let i = 0; i < 2; i++) {
        gl.bindFramebuffer(gl.FRAMEBUFFER, this.pingPongFbos[i]);
        gl.viewport(0, 0, canvas.width, canvas.height);
        gl.clearColor(0, 0, 0, 0);
        gl.clear(gl.COLOR_BUFFER_BIT);
      }

      let firstDraw = true;
      let prevFboIndex = 0;
      let currFboIndex = 1;

      for (const renderLayer of visibleLayers) {
        const ref = this.textures.get(renderLayer.id)!;

        if (firstDraw) {
          // Render directly into FBO 0
          gl.bindFramebuffer(gl.FRAMEBUFFER, this.pingPongFbos[0]);
          gl.viewport(0, 0, canvas.width, canvas.height);

          gl.activeTexture(gl.TEXTURE0);
          gl.bindTexture(gl.TEXTURE_2D, ref.texture);
          gl.uniform1i(this.layerUniforms.texture, 0);

          gl.uniformMatrix4fv(this.layerUniforms.viewProj, false, viewProj);
          gl.uniform1f(this.layerUniforms.opacity, renderLayer.opacity);
          gl.uniform1i(this.layerUniforms.blendMode, 0); // Normal
          gl.uniform1i(this.layerUniforms.useBackdrop, 0); // No backdrop
          gl.uniform1i(this.layerUniforms.flipTexY, 0); // Raw texture, no flip

          // Basic adjustment (Brightness/Contrast/Saturation) as shader uniform.
          const adj = renderLayer.basicAdjustment;
          gl.uniform3f(
            this.layerUniforms.adjustment,
            adj ? adj.brightness : 0,
            adj ? adj.contrast : 0,
            adj ? adj.saturation : 0
          );

          const t = renderLayer.transform;
          const effW = renderLayer.width * Math.abs(t.scaleX);
          const effH = renderLayer.height * Math.abs(t.scaleY);
          const cx = t.x + effW / 2;
          const cy = t.y + effH / 2;
          const flipX = t.flipH ? -1 : 1;
          const flipY = t.flipV ? -1 : 1;

          gl.uniform4f(this.layerUniforms.layerRect, t.x, t.y, effW, effH);
          gl.uniform2f(this.layerUniforms.layerCenter, cx, cy);
          gl.uniform1f(this.layerUniforms.layerRotation, t.rotation || 0);
          gl.uniform2f(this.layerUniforms.flipSign, flipX, flipY);

          gl.drawArrays(gl.TRIANGLES, 0, 6);

          firstDraw = false;
          activeFboIndex = 0;
        } else {
          // Bind FBO currFboIndex as target
          gl.bindFramebuffer(gl.FRAMEBUFFER, this.pingPongFbos[currFboIndex]);
          gl.viewport(0, 0, canvas.width, canvas.height);

          // 1. Copy prevFboIndex texture to currFboIndex
          gl.activeTexture(gl.TEXTURE0);
          gl.bindTexture(gl.TEXTURE_2D, this.pingPongTextures[prevFboIndex]);
          gl.uniform1i(this.layerUniforms.texture, 0);

          // Use an NDC-fullscreen quad for the copy (same pattern as the
          // final screen pass below). The previous pipeline reused the
          // camera viewProj + doc-coord rect — when fit-to-screen leaves
          // padding the doc only covers the central area of the FBO, but
          // the sampler reads the WHOLE source FBO (texCoord 0..1). That
          // squeezed the full source FBO (layer + transparent margins) into
          // the doc-region of the target FBO, shrinking the previous layer
          // by the doc/viewport ratio on every layer above it. Merging
          // hid the bug because a single layer skips this branch entirely.
          const copyProj = this.computeViewMatrix(this.logicalWidth, this.logicalHeight);
          gl.uniformMatrix4fv(this.layerUniforms.viewProj, false, copyProj);
          gl.uniform1f(this.layerUniforms.opacity, 1.0);
          gl.uniform1i(this.layerUniforms.blendMode, 0);
          gl.uniform1i(this.layerUniforms.useBackdrop, 0);
          gl.uniform1i(this.layerUniforms.flipTexY, 1); // FBO texture, flip Y

          // Copy pass must NOT apply adjustment — the source FBO already has
          // the previous layer's adjustment baked in by the composite draw.
          gl.uniform3f(this.layerUniforms.adjustment, 0, 0, 0);

          // Fullscreen quad in logical-viewport coords (matches copyProj)
          const copyQuad = getInterLayerCopyQuad(this.logicalWidth, this.logicalHeight);
          gl.uniform4f(this.layerUniforms.layerRect, ...copyQuad.rect);
          gl.uniform2f(this.layerUniforms.layerCenter, ...copyQuad.center);
          gl.uniform1f(this.layerUniforms.layerRotation, 0);
          gl.uniform2f(this.layerUniforms.flipSign, 1.0, 1.0);

          gl.drawArrays(gl.TRIANGLES, 0, 6);

          // 2. Composite new layer onto currFboIndex using prevFboIndex texture as backdrop
          gl.activeTexture(gl.TEXTURE0);
          gl.bindTexture(gl.TEXTURE_2D, ref.texture);
          gl.uniform1i(this.layerUniforms.texture, 0);

          gl.activeTexture(gl.TEXTURE1);
          gl.bindTexture(gl.TEXTURE_2D, this.pingPongTextures[prevFboIndex]);
          gl.uniform1i(this.layerUniforms.backdrop, 1);

          gl.uniformMatrix4fv(this.layerUniforms.viewProj, false, viewProj);
          gl.uniform1f(this.layerUniforms.opacity, renderLayer.opacity);
          
          // Map blend mode string to ID
          const modeId = this.getBlendModeId(renderLayer.blendMode || "normal");
          gl.uniform1i(this.layerUniforms.blendMode, modeId);
          gl.uniform1i(this.layerUniforms.useBackdrop, 1);
          gl.uniform1i(this.layerUniforms.flipTexY, 0); // Raw layer texture, no flip

          // Basic adjustment for the layer being composited this pass.
          const adj2 = renderLayer.basicAdjustment;
          gl.uniform3f(
            this.layerUniforms.adjustment,
            adj2 ? adj2.brightness : 0,
            adj2 ? adj2.contrast : 0,
            adj2 ? adj2.saturation : 0
          );
          gl.uniform2f(this.layerUniforms.resolution, canvas.width, canvas.height);

          const t = renderLayer.transform;
          const effW = renderLayer.width * Math.abs(t.scaleX);
          const effH = renderLayer.height * Math.abs(t.scaleY);
          const cx = t.x + effW / 2;
          const cy = t.y + effH / 2;
          const flipX = t.flipH ? -1 : 1;
          const flipY = t.flipV ? -1 : 1;

          gl.uniform4f(this.layerUniforms.layerRect, t.x, t.y, effW, effH);
          gl.uniform2f(this.layerUniforms.layerCenter, cx, cy);
          gl.uniform1f(this.layerUniforms.layerRotation, t.rotation || 0);
          gl.uniform2f(this.layerUniforms.flipSign, flipX, flipY);

          gl.drawArrays(gl.TRIANGLES, 0, 6);

          // Unbind TEXTURE1 to prevent an intra-frame feedback loop.
          // After the composite pass, TEXTURE1 holds pingPongTextures[prevFboIndex].
          // The FBO swap below makes prevFboIndex = old currFboIndex, so the
          // next iteration's copy pass would bind the CURRENT FBO's color
          // attachment to TEXTURE1 — WebGL detects this as a feedback loop
          // (even though the copy shader uses useBackdrop=0 and never reads it).
          gl.activeTexture(gl.TEXTURE1);
          gl.bindTexture(gl.TEXTURE_2D, null);
          gl.activeTexture(gl.TEXTURE0);

          activeFboIndex = currFboIndex;
          prevFboIndex = currFboIndex;
          currFboIndex = 1 - currFboIndex;
        }
      }
    }

    // Now render to main screen
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, canvas.width, canvas.height);

    // Clear background
    const [bgR, bgG, bgB, bgA] = state.backgroundColor;
    gl.clearColor(bgR, bgG, bgB, bgA);
    gl.clear(gl.COLOR_BUFFER_BIT);

    // Re-enable BLEND for compositing document content over the background
    gl.enable(gl.BLEND);

    gl.bindVertexArray(this.vao);

    // 1. Render Checkerboard if requested
    if (state.checkerboard && this.checkerboardProgram) {
      gl.useProgram(this.checkerboardProgram);
      gl.uniform2f(this.checkerboardUniforms!.resolution!, canvas.width, canvas.height);
      gl.uniform1f(this.checkerboardUniforms!.checkSize!, 8.0); // 8px grids
      const { color1, color2 } = getCheckerboardColors();
      gl.uniform4f(this.checkerboardUniforms!.color1!, color1[0], color1[1], color1[2], color1[3]);
      gl.uniform4f(this.checkerboardUniforms!.color2!, color2[0], color2[1], color2[2], color2[3]);

      // The checkerboard uses a dedicated fullscreen-quad vertex shader
      // (CHECKERBOARD_VERTEX_SOURCE) that places vertices directly in NDC.
      // No u_layerRect/u_layerCenter/u_viewProj needed — that dependency on
      // the layer program's transform math was the source of the previous
      // bug where the quad ended up outside clip space.
      //
      // The vertex shader fills the full NDC range, so we restrict output
      // to the artboard bounds with a scissor test. This keeps the pasteboard
      // (the area outside the document) clear of the checker pattern, so
      // transparent layer pixels reveal checker inside the artboard only.
      const scissor = projectDocumentScissor(viewProj, docW, docH, canvas.width, canvas.height);
      gl.enable(gl.SCISSOR_TEST);
      gl.scissor(scissor.x, scissor.y, scissor.width, scissor.height);
      gl.drawArrays(gl.TRIANGLES, 0, 6);
      gl.disable(gl.SCISSOR_TEST);
    }

    // 2. Render final FBO onto the checkerboard
    if (visibleLayers.length > 0 && this.layerProgram && this.layerUniforms) {
      gl.useProgram(this.layerProgram);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, this.pingPongTextures[activeFboIndex]);
      gl.uniform1i(this.layerUniforms.texture, 0);

      const identityProj = this.computeViewMatrix(this.logicalWidth, this.logicalHeight);
      gl.uniformMatrix4fv(this.layerUniforms.viewProj, false, identityProj);
      gl.uniform1f(this.layerUniforms.opacity, 1.0);
      gl.uniform1i(this.layerUniforms.blendMode, 0); // Normal
      gl.uniform1i(this.layerUniforms.useBackdrop, 0); // No backdrop
      gl.uniform1i(this.layerUniforms.flipTexY, 1); // FBO texture, flip Y

      // Final screen pass renders the already-composited FBO — must NOT
      // re-apply adjustment (it was applied during the layer composite draws).
      gl.uniform3f(this.layerUniforms.adjustment, 0, 0, 0);

      // Fullscreen quad in viewport coords
      gl.uniform4f(this.layerUniforms.layerRect, 0, 0, this.logicalWidth, this.logicalHeight);
      gl.uniform2f(this.layerUniforms.layerCenter, this.logicalWidth / 2, this.logicalHeight / 2);
      gl.uniform1f(this.layerUniforms.layerRotation, 0);
      gl.uniform2f(this.layerUniforms.flipSign, 1.0, 1.0);

      const scissor = projectDocumentScissor(viewProj, docW, docH, canvas.width, canvas.height);
      gl.enable(gl.SCISSOR_TEST);
      gl.scissor(scissor.x, scissor.y, scissor.width, scissor.height);
      gl.drawArrays(gl.TRIANGLES, 0, 6);
      gl.disable(gl.SCISSOR_TEST);
    }

    gl.bindVertexArray(null);
    const _dt = performance.now() - _t0;
    if (_dt > 5) console.warn(`[perf] WebGL2Backend.render: ${_dt.toFixed(1)}ms (layers=${state.layers.length})`);
  }

  private getBlendModeId(mode: string): number {
    // Delegates to the single source of truth in engine/blendModes so the
    // WebGL preview and the Canvas2D export path can never diverge. The
    // shader's 4-11 modes (darken..exclusion) are intentionally not part of
    // the BlendMode contract and are unreachable here.
    return blendModeToShaderId(mode as BlendMode);
  }

  resize(docWidth: number, docHeight: number, zoom: number, dpr: number): void {
    this.logicalWidth = docWidth * zoom;
    this.logicalHeight = docHeight * zoom;
    let w = Math.round(docWidth * zoom * dpr);
    let h = Math.round(docHeight * zoom * dpr);

    // Clamp backing buffer size to a safe maximum of 4096 to prevent WebGL context loss
    // and memory exhaustion in the browser's GPU process under high zoom.
    const maxLimit = Math.min(4096, this.capabilities.maxTextureSize || 4096);
    if (w > maxLimit || h > maxLimit) {
      const scale = Math.min(maxLimit / w, maxLimit / h);
      w = Math.max(1, Math.round(w * scale));
      h = Math.max(1, Math.round(h * scale));
    }

    if (this.canvas) {
      // Scale pixel buffer by zoom × dpr so visual area = device pixel area (sharp on HiDPI)
      this.canvas.width = w;
      this.canvas.height = h;
    }

    const gl = this.gl;
    if (!gl) return;
    if (this.contextLost || gl.isContextLost()) return;

    if (w !== this.currentWidth || h !== this.currentHeight) {
      this.currentWidth = w;
      this.currentHeight = h;

      // Delete existing ping-pong buffers
      for (let i = 0; i < 2; i++) {
        if (this.pingPongFbos[i]) {
          gl.deleteFramebuffer(this.pingPongFbos[i]);
          this.pingPongFbos[i] = null;
        }
        if (this.pingPongTextures[i]) {
          gl.deleteTexture(this.pingPongTextures[i]);
          this.pingPongTextures[i] = null;
        }
      }

      // Recreate them
      for (let i = 0; i < 2; i++) {
        const texture = gl.createTexture();
        gl.bindTexture(gl.TEXTURE_2D, texture);
        gl.texImage2D(
          gl.TEXTURE_2D,
          0,
          gl.RGBA,
          w,
          h,
          0,
          gl.RGBA,
          gl.UNSIGNED_BYTE,
          null
        );
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

        const fbo = gl.createFramebuffer();
        gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
        gl.framebufferTexture2D(
          gl.FRAMEBUFFER,
          gl.COLOR_ATTACHMENT0,
          gl.TEXTURE_2D,
          texture,
          0
        );

        this.pingPongTextures[i] = texture;
        this.pingPongFbos[i] = fbo;
      }

      gl.bindTexture(gl.TEXTURE_2D, null);
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    }
  }

  resizeToViewport(width: number, height: number, dpr: number): void {
    this.logicalWidth = width;
    this.logicalHeight = height;
    const w = Math.round(width * dpr);
    const h = Math.round(height * dpr);

    if (this.canvas) {
      this.canvas.width = w;
      this.canvas.height = h;
    }

    const gl = this.gl;
    if (!gl) return;
    if (this.contextLost || gl.isContextLost()) return;

    if (w !== this.currentWidth || h !== this.currentHeight) {
      this.currentWidth = w;
      this.currentHeight = h;

      // Delete existing ping-pong buffers
      for (let i = 0; i < 2; i++) {
        if (this.pingPongFbos[i]) {
          gl.deleteFramebuffer(this.pingPongFbos[i]);
          this.pingPongFbos[i] = null;
        }
        if (this.pingPongTextures[i]) {
          gl.deleteTexture(this.pingPongTextures[i]);
          this.pingPongTextures[i] = null;
        }
      }

      // Recreate them
      for (let i = 0; i < 2; i++) {
        const texture = gl.createTexture();
        gl.bindTexture(gl.TEXTURE_2D, texture);
        gl.texImage2D(
          gl.TEXTURE_2D,
          0,
          gl.RGBA,
          w,
          h,
          0,
          gl.RGBA,
          gl.UNSIGNED_BYTE,
          null
        );
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

        const fbo = gl.createFramebuffer();
        gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
        gl.framebufferTexture2D(
          gl.FRAMEBUFFER,
          gl.COLOR_ATTACHMENT0,
          gl.TEXTURE_2D,
          texture,
          0
        );

        this.pingPongTextures[i] = texture;
        this.pingPongFbos[i] = fbo;
      }

      gl.bindTexture(gl.TEXTURE_2D, null);
      gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    }
  }

  readPixel(x: number, y: number): [number, number, number, number] | null {
    const gl = this.gl;
    if (!gl) return null;
    if (this.contextLost || gl.isContextLost()) return null;

    const pixels = new Uint8Array(4);
    gl.readPixels(x, y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    return [pixels[0], pixels[1], pixels[2], pixels[3]];
  }

  getCanvas(): HTMLCanvasElement | null {
    return this.canvas;
  }

  getLogicalWidth(): number {
    return this.logicalWidth;
  }

  getLogicalHeight(): number {
    return this.logicalHeight;
  }

  dispose(): void {
    if (this.canvas) {
      this.removeContextListeners(this.canvas);
    }

    if (this.gl) {
      if (!this.contextLost && !this.gl.isContextLost()) {
        for (const ref of this.textures.values()) {
          this.gl.deleteTexture(ref.texture);
        }

        for (let i = 0; i < 2; i++) {
          if (this.pingPongFbos[i]) this.gl.deleteFramebuffer(this.pingPongFbos[i]);
          if (this.pingPongTextures[i]) this.gl.deleteTexture(this.pingPongTextures[i]);
        }

        if (this.vao) this.gl.deleteVertexArray(this.vao);
        if (this.layerProgram) this.gl.deleteProgram(this.layerProgram);
        if (this.checkerboardProgram) this.gl.deleteProgram(this.checkerboardProgram);
      }

      this.clearGpuResourceRefs();
      this.gl = null;
    }
    this.contextLost = false;
    this.canvas = null;
  }

  // ─── Compile Helpers ───
  private compileShader(type: number, source: string): WebGLShader {
    const gl = this.gl!;
    const shader = gl.createShader(type)!;
    gl.shaderSource(shader, source);
    gl.compileShader(shader);

    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      const log = gl.getShaderInfoLog(shader);
      gl.deleteShader(shader);
      throw new Error(`Shader compile error: ${log}`);
    }
    return shader;
  }

  private createProgram(vs: WebGLShader, fs: WebGLShader): WebGLProgram {
    const gl = this.gl!;
    const program = gl.createProgram()!;
    gl.attachShader(program, vs);
    gl.attachShader(program, fs);
    gl.linkProgram(program);

    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      const log = gl.getProgramInfoLog(program);
      gl.deleteProgram(program);
      throw new Error(`Shader program link error: ${log}`);
    }
    return program;
  }

  private computeViewMatrix(docW: number, docH: number): Float32Array {
    // Identity orthographic projection: map document bounds [0,docW]x[0,docH]
    // directly to NDC [-1,1]x[-1,1]. Pan and zoom are handled entirely by
    // the CSS transform in CanvasViewport — the WebGL canvas renders at 1:1
    // document pixel resolution with no viewport transform applied here.
    const m = new Float32Array(16);
    m[0] = 2.0 / docW;   // scale X: [0, docW] → [-1, 1]
    m[5] = -2.0 / docH;  // scale Y: [0, docH] → [1, -1] (Y flip)
    m[10] = 1.0;
    m[12] = -1.0;         // offset X: center
    m[13] = 1.0;          // offset Y: center
    m[15] = 1.0;
    return m;
  }
}
