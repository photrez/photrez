// SPDX-License-Identifier: AGPL-3.0-or-later
// Module-level WebGL2 helpers for the renderer.
// Extracted from webgl2.ts (report #20 phase 3) — behavior must stay identical
// to the inline helpers it replaces. webgl2.ts re-exports these so existing
// consumers (tests, useViewportRenderer) are unchanged.

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

// ─── Compile Helpers ───
export function compileShader(
  gl: WebGL2RenderingContext,
  type: number,
  source: string,
): WebGLShader {
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

export function createProgram(
  gl: WebGL2RenderingContext,
  vs: WebGLShader,
  fs: WebGLShader,
): WebGLProgram {
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

export function computeViewMatrix(docW: number, docH: number): Float32Array {
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
