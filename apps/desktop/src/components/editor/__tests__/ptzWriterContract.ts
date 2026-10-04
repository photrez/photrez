// apps/desktop/src/components/editor/__tests__/ptzWriterContract.ts
//
// A TypeScript model of the PRODUCTION `.ptz` `document.json` writer.
//
// WHY THIS FILE EXISTS
// The save path no longer builds the payload in the host, so a test that wants
// to inspect the written `document.json` has to stand in for the Rust writer.
// A plain `{...document, format, version}` spread is NOT an acceptable
// stand-in: it is strictly MORE permissive than production, so it would keep
// passing while production silently dropped the field.
//
// THIS IS NOT A SOURCE OF TRUTH. `crates/core/src/ptz_document.rs` is. A
// hand-maintained parallel implementation proves only itself, which is why the
// bytes are pinned the other way round: `ptzWriterGolden.json` holds the REAL
// output of `PtzDocument::to_json` for three fixtures (a real dumped v3 file,
// a four-layer-kind document, and a legacy text layer missing late fields), and
// `ptzWriterGolden.test.ts` diffs this projection against it byte for byte.
// Regenerate the golden only when the writer is intentionally changing:
//
//   cargo test -p photrez-core --lib -- --ignored ptz_writer_golden
//
// So the failure mode is: Rust changes shape -> the Rust golden-up-to-date gate
// fails first, then this projection is updated to match and the diff reviewed.
//
// Mirrors, from the Rust side:
//   - `PtzDocument`               (document + session envelope)
//   - `CanonicalLayer`            (21 per-layer keys)
//   - `canonical_layer_params.rs` (nested defaults + boxMode rule)

import { readFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";

/** The exact document keys `PtzDocument` emits. Anything else is dropped. */
export const PTZ_DOC_KEYS = [
  "activeLayerId", "dirty", "format", "height", "id", "layers",
  "name", "selection", "version", "viewport", "width",
] as const;

/** The exact per-layer keys the typed writer emits (21). */
export const PTZ_LAYER_KEYS = [
  "baseImageBitmap", "basicAdjustment", "blendMode", "hasAdjustments", "height",
  "id", "imageBitmap", "isBackground", "lockPosition", "lockRotation",
  "lockTransparency", "locked", "name", "opacity", "resourceId", "shapeParams",
  "textData", "transform", "type", "visible", "width",
] as const;

const BLEND_MODES = [
  "normal", "multiply", "screen", "overlay", "darken", "lighten",
  "color-dodge", "color-burn", "soft-light", "hard-light", "difference", "exclusion",
];
const LAYER_TYPES = ["raster", "adjustment", "group", "shape", "text"];

/** Layer keys the typed writer always writes as a constant `null`. */
const ALWAYS_NULL_LAYER_KEYS = ["baseImageBitmap", "imageBitmap", "resourceId"];

const TRANSFORM_KEYS = ["flipH", "flipV", "rotation", "scaleX", "scaleY", "x", "y"];
const TRANSFORM_NUMERIC_KEYS = ["rotation", "scaleX", "scaleY", "x", "y"];
const SELECTION_KEYS = ["angle", "height", "inverted", "shape", "width", "x", "y"];
const SELECTION_REQUIRED_KEYS = ["angle", "height", "width", "x", "y"];
const VIEWPORT_KEYS = ["panX", "panY", "rotation", "zoom"];
const VIEWPORT_REQUIRED_KEYS = VIEWPORT_KEYS;

const SHAPE_FILL_KEYS = ["color", "kind"];
const SHAPE_STROKE_KEYS = ["color", "enabled", "width"];
const SHAPE_PARAMS_KEYS = ["arrowHead", "fill", "height", "kind", "radius", "stroke", "width"];
const TEXT_STROKE_KEYS = ["align", "color", "width"];
const TEXT_DATA_KEYS = [
  "align", "boxHeight", "boxMode", "boxWidth", "color", "content",
  "fontFamily", "fontSize", "fontStyle", "fontWeight", "letterSpacing",
  "lineHeight", "strikethrough", "stroke", "underline", "uppercase",
];
const BASIC_ADJUSTMENT_KEYS = ["brightness", "contrast", "saturation"];

// ── Nested defaults, mirroring `Default` impls in canonical_layer_params.rs ──

const DEFAULT_TEXT_DATA: Record<string, unknown> = {
  content: "", fontFamily: "Arial", fontSize: 48, fontWeight: 400,
  fontStyle: "normal", color: "#000000", align: "left", lineHeight: 1.2,
  letterSpacing: 0, boxMode: "point", boxWidth: 0, boxHeight: 0,
  stroke: { width: 0, color: "#000000", align: "outside" },
  underline: false, strikethrough: false, uppercase: false,
};

const DEFAULT_TEXT_STROKE: Record<string, unknown> = {
  width: 0, color: "#000000", align: "outside",
};

const DEFAULT_SHAPE_PARAMS: Record<string, unknown> = {
  kind: "rect", width: 0, height: 0, radius: 0,
  fill: { kind: "none", color: "#000000" },
  stroke: { enabled: false, color: "#000000", width: 1 },
  arrowHead: false,
};

const DEFAULT_SHAPE_FILL: Record<string, unknown> = { kind: "none", color: "#000000" };
const DEFAULT_SHAPE_STROKE: Record<string, unknown> = { enabled: false, color: "#000000", width: 1 };
const DEFAULT_BASIC_ADJUSTMENT: Record<string, unknown> = { brightness: 0, contrast: 0, saturation: 0 };
const DEFAULT_VIEWPORT: Record<string, unknown> = { panX: 0, panY: 0, rotation: 0, zoom: 1 };

/** Thrown with a message naming the offending path, like the Rust writer. */
export class ContractError extends Error {}

function fail(path: string, detail: string): never {
  throw new ContractError(`Document does not match the .ptz format: ${path}: ${detail}`);
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function requireString(src: Record<string, unknown>, key: string, path: string): string {
  if (typeof src[key] !== "string") fail(`${path}.${key}`, "expected a string");
  return src[key] as string;
}

function requireFiniteNumber(src: Record<string, unknown>, key: string, path: string): number {
  const v = src[key];
  if (typeof v !== "number" || !Number.isFinite(v)) fail(`${path}.${key}`, "expected a finite number");
  return v;
}

function requireBoolean(src: Record<string, unknown>, key: string, path: string): boolean {
  if (typeof src[key] !== "boolean") fail(`${path}.${key}`, "expected a boolean");
  return src[key] as boolean;
}

function requireOneOf(
  src: Record<string, unknown>, key: string, allowed: readonly string[], path: string,
): string {
  if (!allowed.includes(src[key] as string)) {
    fail(`${path}.${key}`, `unknown value ${JSON.stringify(src[key])}`);
  }
  return src[key] as string;
}

/**
 * Project a nested payload to the exact key set the typed writer emits,
 * filling absent fields with the Rust `Default` values (NOT null — the container
 * `Default` means an absent nested field becomes a concrete default).
 * A whole-payload `null`/absent stays `null`.
 */
function projectNested(
  value: unknown,
  keys: readonly string[],
  defaults: Record<string, unknown>,
  path: string,
): Record<string, unknown> | null {
  if (value === null || value === undefined) return null;
  if (!isRecord(value)) fail(path, "expected an object or null");
  const out: Record<string, unknown> = {};
  for (const key of keys) {
    const v = value[key];
    out[key] = v === undefined ? defaults[key] : v;
  }
  return out;
}

/**
 * Mirrors the custom `Deserialize` on `TextData`: box dimensions follow
 * `boxMode` (point forces both to 0; area floors boxWidth at 1), and the three
 * optional booleans normalise an absent OR explicit `null` to `false`.
 */
function applyTextDataRules(td: Record<string, unknown>, src: Record<string, unknown>): void {
  const boxMode = td.boxMode;
  if (boxMode === "point") {
    td.boxWidth = 0;
    td.boxHeight = 0;
  } else {
    const bw = src.boxWidth;
    td.boxWidth = typeof bw === "number" ? Math.max(1, bw) : 1;
    const bh = src.boxHeight;
    td.boxHeight = typeof bh === "number" ? Math.max(0, bh) : 0;
  }
  for (const key of ["underline", "strikethrough", "uppercase"]) {
    if (typeof src[key] !== "boolean") td[key] = false;
  }
}

const TEXT_ALIGNMENTS = ["left", "center", "right"];
const TEXT_BOX_MODES = ["point", "area"];
const TEXT_FONT_STYLES = ["normal", "italic"];
const TEXT_STROKE_ALIGNS = ["outside", "center", "inside"];

function projectTextData(value: unknown, path: string): Record<string, unknown> | null {
  if (value === null || value === undefined) return null;
  if (!isRecord(value)) fail(path, "expected an object or null");
  // A present-but-invalid enum must be rejected, not defaulted: Rust's
  // `#[serde(default)]` covers an ABSENT field only, so each is validated only
  // when actually present.
  if (value.boxMode !== undefined) requireOneOf(value, "boxMode", TEXT_BOX_MODES, path);
  if (value.align !== undefined) requireOneOf(value, "align", TEXT_ALIGNMENTS, path);
  if (value.fontStyle !== undefined) requireOneOf(value, "fontStyle", TEXT_FONT_STYLES, path);
  if (isRecord(value.stroke) && value.stroke.align !== undefined) {
    requireOneOf(value.stroke, "align", TEXT_STROKE_ALIGNS, `${path}.stroke`);
  }
  const td = projectNested(value, TEXT_DATA_KEYS, DEFAULT_TEXT_DATA, path)!;
  td.stroke = projectNested(
    value.stroke,
    TEXT_STROKE_KEYS, DEFAULT_TEXT_STROKE, `${path}.stroke`,
  ) ?? { ...DEFAULT_TEXT_STROKE };
  applyTextDataRules(td, value);
  return td;
}

function projectShapeParams(value: unknown, path: string): Record<string, unknown> | null {
  if (value === null || value === undefined) return null;
  if (!isRecord(value)) fail(path, "expected an object or null");
  const sp = projectNested(value, SHAPE_PARAMS_KEYS, DEFAULT_SHAPE_PARAMS, path)!;
  requireOneOf(value, "kind", [
    "rect", "ellipse", "line", "triangle", "star",
    "block-arrow", "heart", "diamond", "speech-bubble", "hexagon",
  ], path);
  sp.fill = projectNested(value.fill, SHAPE_FILL_KEYS, DEFAULT_SHAPE_FILL, `${path}.fill`)
    ?? { ...DEFAULT_SHAPE_FILL };
  sp.stroke = projectNested(value.stroke, SHAPE_STROKE_KEYS, DEFAULT_SHAPE_STROKE, `${path}.stroke`)
    ?? { ...DEFAULT_SHAPE_STROKE };
  return sp;
}

function projectTransform(value: unknown, path: string): Record<string, unknown> | null {
  if (value === null || value === undefined) return null;
  if (!isRecord(value)) fail(path, "expected an object or null");
  // Rust's `Transform2D` requires ALL seven fields -- no serde defaults.
  for (const key of TRANSFORM_NUMERIC_KEYS) requireFiniteNumber(value, key, path);
  for (const key of ["flipH", "flipV"]) requireBoolean(value, key, path);
  const out: Record<string, unknown> = {};
  for (const key of TRANSFORM_KEYS) out[key] = value[key];
  return out;
}

function projectSelection(value: unknown, path: string): Record<string, unknown> | null {
  if (value === null || value === undefined) return null;
  if (!isRecord(value)) fail(path, "expected an object or null");
  // Rust's `SelectionState` requires x/y/width/height/angle.
  for (const key of SELECTION_REQUIRED_KEYS) requireFiniteNumber(value, key, path);
  const out: Record<string, unknown> = {};
  for (const key of SELECTION_KEYS) out[key] = value[key] === undefined ? null : value[key];
  return out;
}

function projectViewport(value: unknown, path: string): Record<string, unknown> {
  if (value === null || value === undefined) return { ...DEFAULT_VIEWPORT };
  if (!isRecord(value)) fail(path, "expected an object or null");
  for (const key of VIEWPORT_REQUIRED_KEYS) requireFiniteNumber(value, key, path);
  const out: Record<string, unknown> = {};
  for (const key of VIEWPORT_KEYS) out[key] = value[key];
  return out;
}

function projectLayer(raw: unknown, index: number): Record<string, unknown> {
  const path = `layers[${index}]`;
  if (!isRecord(raw)) fail(path, "expected an object");

  requireString(raw, "id", path);
  requireString(raw, "name", path);
  requireOneOf(raw, "type", LAYER_TYPES, path);
  requireBoolean(raw, "visible", path);
  requireBoolean(raw, "locked", path);
  requireOneOf(raw, "blendMode", BLEND_MODES, path);
  for (const key of ["width", "height", "opacity"]) requireFiniteNumber(raw, key, path);

  // Typed projection: build the output from the KNOWN key set only, so a field
  // on `DocumentModel`/`LayerNode` with no counterpart here is dropped exactly
  // as the Rust writer drops it.
  const out: Record<string, unknown> = {};
  for (const key of PTZ_LAYER_KEYS) {
    if (ALWAYS_NULL_LAYER_KEYS.includes(key)) {
      out[key] = null;
    } else if (key === "transform") {
      out[key] = projectTransform(raw.transform, `${path}.transform`);
    } else if (key === "shapeParams") {
      out[key] = projectShapeParams(raw.shapeParams, `${path}.shapeParams`);
    } else if (key === "textData") {
      out[key] = projectTextData(raw.textData, `${path}.textData`);
    } else if (key === "basicAdjustment") {
      out[key] = projectNested(
        raw.basicAdjustment, BASIC_ADJUSTMENT_KEYS, DEFAULT_BASIC_ADJUSTMENT,
        `${path}.basicAdjustment`,
      );
    } else {
      // Absent optionals become an explicit null, matching the writer.
      out[key] = raw[key] === undefined ? null : raw[key];
    }
  }
  return out;
}

/**
 * A number that must be written as an integer literal. `PTZ_VERSION` is a `u32`
 * in Rust, so it serializes as `3`, while every f64 field gets a `.0` suffix.
 * Without this the projection would write `"version":3.0`.
 */
class RawInt {
  constructor(readonly value: number) {}
}

/**
 * Format a number the way `serde_json` formats an `f64` in the range a document
 * can hold: always with a decimal point, so an integral value prints `300.0`
 * rather than `300`. Non-integral values use the shortest round-trip form, which
 * is what JavaScript's own `Number#toString` already produces.
 *
 * SCOPE: the exponential branch is passed through unchanged, so a value at or
 * beyond 1e21 would keep JavaScript's `1e+21` where ryu emits `1e21`. That is
 * unreachable for a real document -- `MAX_CANVAS_DIM` in types.ts caps
 * dimensions at 16384, and `opacity` and zoom are far smaller -- so this
 * documents the limit rather than implementing ryu's exponent formatting.
 */
function formatRustNumber(n: number): string {
  if (!Number.isFinite(n)) throw new ContractError(`non-finite number in document: ${n}`);
  if (Object.is(n, -0)) return "-0.0";
  const s = String(n);
  // See SCOPE: only reachable outside a document's value range.
  if (s.includes("e") || s.includes("E")) return s;
  return s.includes(".") ? s : `${s}.0`;
}

/**
 * Serialize like `serde_json::to_string` over a `BTreeMap`-backed `Map`:
 * object keys are emitted in ALPHABETICAL order at every level.
 */
function serializeRustJson(value: unknown): string {
  if (value === null) return "null";
  if (value instanceof RawInt) return String(value.value);
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") return formatRustNumber(value);
  if (typeof value === "string") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(serializeRustJson).join(",")}]`;
  const obj = value as Record<string, unknown>;
  const keys = Object.keys(obj).sort();
  return `{${keys.map((k) => `${JSON.stringify(k)}:${serializeRustJson(obj[k])}`).join(",")}}`;
}

/**
 * Project a document exactly as the production Rust writer does, and return the
 * `document.json` bytes it would write.
 *
 * Throws `ContractError` on an out-of-contract document, mirroring the Rust
 * writer's `E_VALIDATION`.
 */
export function emulateRustSerialization(document: unknown): string {
  if (!isRecord(document)) fail("document", "expected an object");

  requireString(document, "id", "document");
  requireString(document, "name", "document");
  for (const key of ["width", "height"]) requireFiniteNumber(document, key, "document");
  if (!Array.isArray(document.layers)) fail("layers", "expected an array");

  const out: Record<string, unknown> = {
    id: document.id,
    name: document.name,
    width: document.width,
    height: document.height,
    layers: (document.layers as unknown[]).map(projectLayer),
    activeLayerId: document.activeLayerId === undefined ? null : document.activeLayerId,
    selection: projectSelection(document.selection, "selection"),
    viewport: projectViewport(document.viewport, "viewport"),
    dirty: typeof document.dirty === "boolean" ? document.dirty : false,
    // Header keys belong to the writer, never the host.
    format: "photrez-ptz",
    version: new RawInt(3),
  };
  return serializeRustJson(out);
}

// ── The golden file: the real Rust writer's bytes ──

function repoFile(rel: string): string {
  // vitest runs with cwd=apps/desktop; also accept a repo-root cwd.
  const candidates = [
    resolve(process.cwd(), "..", "..", rel),
    resolve(process.cwd(), rel),
    resolve(process.cwd(), "..", rel),
  ];
  const found = candidates.find((p) => existsSync(p));
  if (!found) throw new Error(`cannot locate ${rel}; tried:\n  ${candidates.join("\n  ")}`);
  return found;
}

export interface GoldenCase {
  /** The fixture the Rust writer was given. */
  input: string;
  /** The bytes `PtzDocument::to_json` actually produced. */
  expected: string;
}

/** Load the golden cases produced by the Rust writer. */
export function loadWriterGolden(): Record<string, GoldenCase> {
  const path = repoFile("apps/desktop/src/components/editor/__tests__/ptzWriterGolden.json");
  return JSON.parse(readFileSync(path, "utf8")) as Record<string, GoldenCase>;
}

/**
 * Self-check that the projection stays NARROW (drops unmodelled keys, rejects
 * out-of-contract input). This is deliberately NOT called "matches the
 * production shape" -- it cannot verify that, because it compares the
 * projection against its own key lists. Agreement with the real writer is
 * proven by diffing the golden bytes, in `ptzWriterGolden.test.ts`.
 */
export function assertProjectionStaysNarrow(): void {
  const doc = {
    id: "d", name: "n", width: 1, height: 1, dirty: false,
    layers: [{
      id: "l", name: "L", type: "raster", visible: true, opacity: 1, locked: false,
      blendMode: "normal", width: 1, height: 1,
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
    }],
  };
  const written = JSON.parse(emulateRustSerialization(doc));
  expectKeySet(written, PTZ_DOC_KEYS, "document");
  expectKeySet(written.layers[0], PTZ_LAYER_KEYS, "layer");

  // An unmodelled key is dropped, which is the regression the old spread hid.
  const withExtra = JSON.parse(emulateRustSerialization({ ...doc, someNewField: 42 }));
  expectKeySet(withExtra, PTZ_DOC_KEYS, "document with an extra key");

  // Out-of-contract input must be rejected, not written.
  for (const bad of [
    { ...doc, layers: [{ ...doc.layers[0], opacity: "opaque" }] },
    { ...doc, layers: [{ ...doc.layers[0], blendMode: "not-a-mode" }] },
    { ...doc, layers: [{ ...doc.layers[0], type: "not-a-type" }] },
    { ...doc, selection: { x: 1 } },
    { ...doc, layers: [{ ...doc.layers[0], transform: { x: 0 } }] },
    { ...doc, layers: [{ ...doc.layers[0], textData: { content: "x", boxMode: "sideways" } }] },
  ]) {
    let threw = false;
    try {
      emulateRustSerialization(bad);
    } catch (err) {
      threw = err instanceof ContractError;
    }
    if (!threw) throw new Error(`projection must reject: ${JSON.stringify(bad).slice(0, 80)}`);
  }
}

function expectKeySet(value: Record<string, unknown>, expected: readonly string[], what: string): void {
  const got = Object.keys(value).sort();
  const want = [...expected].sort();
  if (got.join(",") !== want.join(",")) {
    throw new Error(`${what} key set drifted: got [${got}] want [${want}]`);
  }
}
