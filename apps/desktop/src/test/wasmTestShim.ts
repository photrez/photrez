// Vitest-only wasm loader shim.
//
// The wasm-bindgen "web" target initializes via `fetch(new URL('…bg.wasm',
// import.meta.url))` — Node/vitest `fetch` cannot read `file:` URLs, so the
// dynamic import inside getWasmExportModule fails headless ("fetch failed").
// This shim loads the SAME .wasm bytes from disk and initializes via the pkg's
// own `initSync`, then re-exports the real module namespace. Result: every
// test exercises the REAL Rust engine (graph ops, kernels) instead of TS
// fallbacks or silent no-ops.
//
// Wired via resolve.alias in vite.config.ts (both vitest projects) so the
// specifier "@/wasm/pkg/photrez_core" resolves here during tests only.
// Production code paths keep the real pkg.
import fs from "node:fs";
import { dirname, resolve } from "node:path";
import {
  initSync,
  Engine,
  DocumentEngine,
  BrushStrokeEngine,
  rgba_buffer_view,
  free_rgba_buffer,
  alloc_rgba_buffer,
  flood_fill_wasm,
  gradient_fill_wasm,
  apply_basic_adjustment_wasm,
  encode_image_wasm,
  png_dimensions_wasm,
  decode_png_into_wasm,
  protocol_contract_version,
  protocol_apply_command,
  protocol_snapshot_json,
  protocol_reset,
  protocol_version,
  protocol_register_payload_adapter,
  protocol_history_query_json,
  protocol_history_cursor_commit,
  protocol_seed_canonical,
  canonical_validate_json,
} from "../wasm/pkg/photrez_core.js";

// Locate the .wasm on disk by walking up from cwd. Do NOT derive from
// import.meta.url: in the component-jsdom project modules are served via
// http://localhost, so fileURLToPath(new URL(...)) throws ERR_INVALID_URL_SCHEME.
//
// Staleness guard: ONLY the apps/desktop wasm pkg is authoritative for the
// test suite. A repo-root src/wasm/pkg can exist and be STALE (a build
// artifact from before the last Rust change), so never fall back to it —
// silently loading a stale binary would let tests pass against an old wire
// contract. Anchor the search to apps/desktop/src/wasm/pkg and walk up from
// cwd to reach the repo root (vitest runs with cwd = apps/desktop).
function findWasmPath(): string {
  let dir = process.cwd();
  for (let i = 0; i < 8; i++) {
    const candidate = resolve(
      dir,
      "apps/desktop/src/wasm/pkg/photrez_core_bg.wasm",
    );
    if (fs.existsSync(candidate)) return candidate;
    dir = dirname(dir);
  }
  throw new Error(
    "apps/desktop/src/wasm/pkg/photrez_core_bg.wasm not found — run `bun run build:wasm` first",
  );
}
const bytes = fs.readFileSync(findWasmPath());
initSync({ module: new WebAssembly.Module(bytes) });

// getWasmExportModule awaits mod.default() when it is a function — already
// initialized above, so provide a no-op async init.
export default async function init(): Promise<void> {}

export {
  Engine,
  DocumentEngine,
  BrushStrokeEngine,
  rgba_buffer_view,
  free_rgba_buffer,
  alloc_rgba_buffer,
  flood_fill_wasm,
  gradient_fill_wasm,
  apply_basic_adjustment_wasm,
  encode_image_wasm,
  png_dimensions_wasm,
  decode_png_into_wasm,
  protocol_contract_version,
  protocol_apply_command,
  protocol_snapshot_json,
  protocol_reset,
  protocol_version,
  protocol_register_payload_adapter,
  protocol_history_query_json,
  protocol_history_cursor_commit,
  protocol_seed_canonical,
  canonical_validate_json,
};
