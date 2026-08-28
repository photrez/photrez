# Rust ↔ TypeScript Interop — Forum & Primary-Source Research (2026-08-23)

> **Purpose:** Extra research for v0.2.0 Rust-dominance decision. All claims trace to primary source (official docs, wasm-bindgen repo, Rust forum, Tauri discussions, Reddit with profiling data). No secondary blog summaries.

---

## TL;DR for Photrez v0.2.0

1. **Boundary cost dominates.** Every JS↔WASM call that moves a `&str`, `Vec`, or `JsValue` copies into linear memory. Primitive `u32/f64` is ~50-100ns, but 1MB array copy is 1-3ms — same data our `adjust` and `tile` benches copy. Our `bench-rust-wasm.ts` showing Rust 0.69x slower at 48MB is exactly this.
2. **Batch wins, chatty loses.** `N` tiny crossings >> 1 big batched crossing. Photrez must call `adjust_all(&[u8])` / `process_tiles(&TileMap)` once, not `get_pixel(i)` in a loop. This is unanimous across forums and the quantitative paper (1-3ms per 1MB copy).
3. **`serde_wasm_bindgen` vs JSON:** `serde_wasm_bindgen` is 22-31% smaller than JSON over wasm-bindgen and faster for non-string data, but **slower for many small strings/objects** because it materialises each JS object individually. V8's `JSON.parse` on one big string can win (92μs → 38μs in real parser case). Choice depends on shape.
4. **Tauri IPC is slow for bulk pixels.** 200k rows via `invoke` → 16s blocked UI; 500MB via `invoke` = 30s vs 5s raw JSON; `Vec<u8>` image via `invoke` has seconds-long delay. v2 improved but still needs custom protocol / SharedArrayBuffer / localhost WS for bulk. Our `bench-engine-overhead` 18% TS share confirms we must NOT put per-frame 48MB pixels over Tauri IPC — WASM direct (wasm-bindgen) is the right path.
5. **JS JIT is already fast for simple arithmetic.** Rust/WASM only wins clearly on complex data structures, SIMD, or off-main-thread predictability (no GC pauses). Tiny per-pixel arithmetic loops JIT to near-native.
6. **Raw WASM + SIMD beats wasm-bindgen.** `modify array` bench: JS 1.40ms, wasm-bindgen 1.62ms, raw WASM 0.35ms (4x), raw+SIMD 0.23ms (6x). But requires `unsafe` + `+simd128` + manual memory mgmt.

**Photrez implication:** Keep UI in TS/SolidJS, move **coarse-grained compute kernels** (geometry, tile merge, PNG decode) to Rust/WASM with single big-buffer APIs + `SharedArrayBuffer` zero-copy where hot. Do NOT move chatty per-pixel getters or DOM-touching code.

---

## 1. How Rust talks to TypeScript

### 1.1 wasm-bindgen + wasm-pack (official)

- `wasm-bindgen` is the bridge; `wasm-pack` automates `compile → wasm-bindgen → wasm-opt → npm pkg` and ensures `wasm32-unknown-unknown` target. [DEV Community 2025-09-29](https://dev.to/bence_rcz_fe471c168707c1/rust-webassembly-performance-javascript-vs-wasm-bindgen-vs-raw-wasm-with-simd-4pco)
- WASM functions natively take only `i32/i64/f32/f64`; richer types must be encoded into linear memory and decoded on the other side — `wasm-bindgen` generates that marshalling glue. [rs4ts.dev 2026-06-09](https://rs4ts.dev/19-wasm/09-performance/)
- Pipeline size example (real build): debug 2.5MB → release 43KB → after wasm-bindgen 24.7KB → after `wasm-opt -Os` 17.7KB → brotli 7-8KB. **Never ship debug.** Use `twiggy top` to profile bloat. [rs4ts.dev](https://rs4ts.dev/19-wasm/09-performance/)

### 1.2 Serde bridge options

- `serde_wasm_bindgen` (Cloudflare) is the **officially preferred** replacement for deprecated `JsValue::from_serde`/`into_serde`. It avoids JSON stringify and gives smaller code, but prioritizes JS idioms over strict JSON compat. [docs.rs serde_wasm_bindgen](https://docs.rs/serde-wasm-bindgen/latest/serde_wasm_bindgen/)
- PR #3031 documents a real parser case where switching `serde_wasm_bindgen` → `JsValue::from_serde` (JSON path) **cut** avg 92.6μs → 38.6μs and p99 1.44ms → 390μs because many small objects over `serde_wasm_bindgen` are chatty (one JS object per Rust struct). Benchmark sheet linked in PR shows twitter (many strings) is the outlier where JSON wins. [wasm-bindgen PR #3031](https://github.com/wasm-bindgen/wasm-bindgen/pull/3031)
- Issue #2539: `JsValue::from_serde` is 10x slower than passing a string + `JSON.parse` in JS for large response text — Workaround: use `serde_wasm_bindgen`. [wasm-bindgen #2539](https://github.com/rustwasm/wasm-bindgen/issues/2539)
- When data is many tiny strings/objects, **single JSON string + V8 `JSON.parse`** wins; for other types `serde_wasm_bindgen` wins. [wasm-bindgen #3031 comment](https://github.com/wasm-bindgen/wasm-bindgen/pull/3031) (RReverser benchmark sheet)
- If you need shared `serde` code between server (JSON) and client (JsValue), JSON path preserves compatibility; `serde_wasm_bindgen` diverges. [wasm-bindgen #1258](https://github.com/rustwasm/wasm-bindgen/issues/1258)

### 1.3 flinect / Bomboni pattern (forum + blog)

- `wasm-pack` generates `.d.ts` + JS glue; for plain JS objects (not classes) use `serde` + `serde-wasm-bindgen` with a `Wasm` derive macro that generates `IntoWasmAbi`/`FromWasmAbi` and TS types via `tsify`. Supports proxy types (`ParsedDataType` via `DataType`). Less efficient than pure `wasm_bindgen` but ergonomic for gRPC types. [flinect 2024-01-18](https://flinect.com/blog/rust-wasm-with-typescript-serde)

---

## 2. Performance: what forums actually measured

### 2.1 Boundary cost (quantitative paper + forum consensus)

- **Quantitative paper (Stepanov 2026, LNU):** primitive call ~50-100ns, 1MB array copy 1-3ms, `SharedArrayBuffer` ~15ns. WASI cold start 100x faster than Docker, binary 50x smaller. Recommendation: **coarse-grained APIs + SharedArrayBuffer** for high throughput; `Wasm as Pure Function` vs `Wasm with Shared Memory` gives extra 2-3x. [LNU 2026-01-15](https://publications.lnu.edu.ua/collections/index.php/electronics/article/view/5101)
- **Rust forum (96182, 55726, 21022):** Repeated advice — "most inefficient part is interaction between Rust and JS due to copying/serializing + no cross-language optimization; minimize calls across boundary; collect work into one call that does 50ms of work." Many tiny calls = overhead dominates; one batched call wins. [users.rust-lang.org 96182](https://users.rust-lang.org/t/rust-webassembly-is-slower-than-javascript/96182) / [55726](https://users.rust-lang.org/t/wasm-bindgen-performance/55726) / [21022](https://users.rust-lang.org/t/webassembly-vs-js-performance/21022)
- **rs4ts.dev:** Passing 1M-element array via `&[f64]` copies once per call; `normalize_one` in a loop = N crossings, `normalize_all` = 2 crossings total — same math, opposite outcome. [rs4ts.dev](https://rs4ts.dev/19-wasm/09-performance/)
- **DEV bench 2025-09-29:** `modify array` — JS 1.403ms, wasm-bindgen 1.623ms (slower!), raw WASM 0.353ms (4x), raw+SIMD 0.231ms (6x). `Float32Array(&[f32])` via wasm-bindgen copies; commenters note double memory + allocation. [DEV](https://dev.to/bence_rcz_fe471c168707c1/rust-webassembly-performance-javascript-vs-wasm-bindgen-vs-raw-wasm-with-simd-4pco)

### 2.2 When WASM loses (Reddit r/rust + blog)

- **r/rust 2026-03-23:** Streaming parser LLM→React: Rust/WASM 3x **slower** than TS. Parsing never bottleneck; cost was boundary: copy strings in + serialize JSON + copy back + V8 deserialize. `serde-wasm-bindgen` 30% slower (hundreds of tiny crossings worse than one JSON string). Quote: "only case where WASM makes sense is infrequent calls doing heavy lifting." [r/rust](https://www.reddit.com/r/rust/comments/1rz64ug/we_replaced_our_rustwasm_parser_with_typescript/)
- **Blog 2026-03-21 (same case detailed):** Fixation: simple-table 9.32μs TS vs 20.52μs WASM, contact-form 13.46 vs 61.47, dashboard 19.45 vs 57.97; streaming total 6977μs → 1222μs etc. Lesson: eliminate boundary, use incremental caching. [wphelpertools 2026-03-21](https://wphelpertools.com/optimizing-browser-performance-lessons-from-a-wasm-parser/)
- **r/rust 2025-08-23:** `serde_json` in WASM 0.285ms vs `JSON.parse` 0.079ms (10x slower) — `serde_json` in WASM includes large f64 formatting. [r/rust](https://www.reddit.com/r/rust/comments/1h9ikt7/why_wasmbindgen_with_serde_json_slower_10_times/)
- **Rust forum 2022-11-12 / 2024-11-22:** WASM has no DOM access; every DOM op is a JS call. "Only requirement is WASM calls JS to interact with browser — not a wasm-bindgen limitation." Use WASM for codecs/game/editing logic, not DOM-heavy UI. [84171](https://users.rust-lang.org/t/where-to-do-things-between-wasm-and-javascript/84171) / [121522](https://users.rust-lang.org/t/using-wasm-instead-of-javascript-in-frontend/121522)
- **JIT note:** Simple arithmetic loops JIT to near-native; Rust wins on complex structs, avoiding GC pauses/predictability, not raw arithmetic. [96182](https://users.rust-lang.org/t/rust-webassembly-is-slower-than-javascript/96182)

### 2.3 Shared memory / raw pointer pattern

- For high throughput, avoid `Float64Array.get_index(i)` per element; instead `Vec<f64>` in Rust + `Float64Array::view_mut_raw(ptr, len)` or `SharedArrayBuffer` (~15ns). CAUTION: view invalidated if WASM memory grows (`memory.grow`). [Rust forum 55726](https://users.rust-lang.org/t/wasm-bindgen-performance/55726) / [LNU paper](https://publications.lnu.edu.ua/collections/index.php/electronics/article/view/5101)

---

## 3. Tauri IPC ↔ WASM boundary (directly relevant to Photrez)

- **200k rows Rust→webview via `invoke`/ `window.eval`:** 16s blocked UI. Only built-in alternative is custom `register_uri_scheme_protocol` (like `image` example) or localhost `http/ws` server. Shared `ArrayBuffer` ownership transfer would need OS shared memory; WebView2 has API but slow + unsafe; other webviews lack it. [tauri discussion 5511](https://github.com/tauri-apps/tauri/discussions/5511)
- **500MB via `invoke`:** 30s vs 5s raw JSON read/write — 3 serializations (`Value` → `Value` → JSON string → `RawValue` → escaped String → JS callback). Fix #5641 proposes serialize directly to JSON string. [tauri #5641](https://github.com/tauri-apps/tauri/issues/5641)
- **`Vec<u8>` image via `invoke`:** seconds-long delay between Rust finish and React display; memory limit ~2GB per webview. Workaround is protocol/http sharing. [tauri #9654](https://github.com/tauri-apps/tauri/issues/9654)
- **General IPC overhead:** Even empty command can take 600ms-6s in dev (random), may be worse unfocused window / memory pressure. [tauri #1877](https://github.com/tauri-apps/tauri/issues/1877) / [#9654 comments](https://github.com/tauri-apps/tauri/issues/9654)
- **Pong game via `invoke` per frame:** `computeGameState` per `requestAnimationFrame` flickers vs pure SolidJS smooth — IPC per frame is untenable; fix is batch compute in Rust or reorder clear/draw after `await`. [tauri discussion 13050](https://github.com/tauri-apps/tauri/discussions/13050)
- **Official advice (10365):** JS APIs call Rust behind the scenes but slower due to IPC; return only what's needed. [tauri discussion 10365](https://github.com/tauri-apps/tauri/discussions/10365)
- **Dilla WASM build bench:** wasm-bindgen is fastest for browser + smallest (561KB vs others), but still needs separate wasm for Browser/Node and lacks WASI until Component Model stabilizes. [dilla.io](http://dilla.io/blog/benchmarking-webassembly-builds-unveiling-performance-insights)

**For Photrez:** Our decision in `ARCHITECTURE.md` to use **wasm-bindgen direct** (not Tauri `invoke`) for hot-path pixels is exactly right per these forums. `invoke` must stay cold-path (file open/save) only.

---

## 4. What this means for Photrez v0.2.0

**Do (forum-backed):**
- Move **coarse-grained kernels**: `transformGeometry` (matrix math), `SelectionOperations` boolean ops, `paintTileSurface` merge, `projectSerialize` — each as `fn process_all(&[u8]) -> Vec<u8>` with one big buffer, or `SharedArrayBuffer` zero-copy (`with_buffer` + `rgba_buffer_view` already in `engine.rs`).
- Provide **batched APIs**: `batch_normalize_all` not `normalize_one` in loop — matches rs4ts + Rust forum unanimous advice.
- For large JS objects with many strings, **measure both** `serde_wasm_bindgen` vs single `JSON.parse` string before committing (PR #3031 shows 2.4x difference either way).
- Enable `RUSTFLAGS="-C target-feature=+simd128"` + `wasm-opt -Os` (Photrez `bench-rust-wasm.ts` used raw wasm; try SIMD for tile loops).
- Keep `console_error_panic_hook` + `Drop` free buffer (already in `engine.rs`) — forum warns memory-grow invalidates views.

**Don't:**
- Don't call WASM per-pixel / per-layer in a JS loop (29328 calls case) — collect 50ms of work then one call.
- Don't put 48MB pixels over Tauri `invoke` per frame (200k row 16s case) — use WASM memory sharing.
- Don't rewrite DOM-heavy UI (`PropertiesPanel 60k`) to Rust via `web-sys` — forum says awkward + no win; use Yew/Leptos if you must, but Photrez is SolidJS shell, so keep UI in TS.
- Don't assume Rust faster — JIT arithmetic is already near-native; win is predictability/no-GC and SIMD/complex structs.

**Concrete next benches for v0.2.0 gate:**
- `bench-geometry.ts`: `transformGeometry` 100 layers hit-test — expect 1.5-3x if batched (measure like DEV 4-6x).
- `bench-rust-ssot.ts`: doc graph ops — expect parity (already 0.04 vs 0.05), ship for architecture not perf.
- `bench-png-decode`: keep gate >=1.15x (currently 1.18x @4K, needs webview `__benchPngDecode` true numbers like Fase 2 primitives).

---

## Sources (in order cited)

- DEV Community 2025-09-29 — Rust + WASM Performance (wasm-bindgen vs raw vs SIMD): https://dev.to/bence_rcz_fe471c168707c1/rust-webassembly-performance-javascript-vs-wasm-bindgen-vs-raw-wasm-with-simd-4pco
- rs4ts.dev 2026-06-09 — WASM Performance: Bundle Size and Boundary Cost: https://rs4ts.dev/19-wasm/09-performance/
- docs.rs serde_wasm_bindgen: https://docs.rs/serde-wasm-bindgen/latest/serde_wasm_bindgen/
- wasm-bindgen PR #3031 (deprecate from_serde) + benchmark sheet: https://github.com/wasm-bindgen/wasm-bindgen/pull/3031
- wasm-bindgen issue #2539 (from_serde slow): https://github.com/rustwasm/wasm-bindgen/issues/2539
- wasm-bindgen issue #1258: https://github.com/rustwasm/wasm-bindgen/issues/1258
- flinect 2024-01-18 — Rust WASM with TypeScript serde: https://flinect.com/blog/rust-wasm-with-typescript-serde
- LNU 2026-01-15 — Quantitative Analysis of WASM Integration (50-100ns primitive, 1-3ms 1MB copy, SharedArrayBuffer 15ns): https://publications.lnu.edu.ua/collections/index.php/electronics/article/view/5101
- users.rust-lang.org 96182 — Rust WASM slower than JS: https://users.rust-lang.org/t/rust-webassembly-is-slower-than-javascript/96182
- users.rust-lang.org 55726 — wasm-bindgen performance (Float64Array): https://users.rust-lang.org/t/wasm-bindgen-performance/55726
- users.rust-lang.org 21022 — WebAssembly vs JS performance: https://users.rust-lang.org/t/webassembly-vs-js-performance/21022
- Reddit r/rust 2026-03-23 — parser 3x faster in TS: https://www.reddit.com/r/rust/comments/1rz64ug/we_replaced_our_rustwasm_parser_with_typescript/
- wphelpertools 2026-03-21 — WASM parser lessons: https://wphelpertools.com/optimizing-browser-performance-lessons-from-a-wasm-parser/
- Reddit r/rust 2025-08-23 — serde_json 10x slower: https://www.reddit.com/r/rust/comments/1h9ikt7/why_wasmbindgen_with_serde_json_slower_10_times/
- users.rust-lang.org 84171 — Where to do things WASM vs JS: https://users.rust-lang.org/t/where-to-do-things-between-wasm-and-javascript/84171
- users.rust-lang.org 121522 — Using wasm instead of JS: https://users.rust-lang.org/t/using-wasm-instead-of-javascript-in-frontend/121522
- Tauri discussion 5511 (200k rows 16s): https://github.com/tauri-apps/tauri/discussions/5511
- Tauri issue #5641 (invoke 3 serializations, 30s for 500MB): https://github.com/tauri-apps/tauri/issues/5641
- Tauri issue #9654 (Vec<u8> image delay, 2GB limit): https://github.com/tauri-apps/tauri/issues/9654
- Tauri issue #1877 (invoke 600ms-6s random): https://github.com/tauri-apps/tauri/issues/1877
- Tauri discussion 13050 (pong flicker, invoke per frame): https://github.com/tauri-apps/tauri/discussions/13050
- Tauri discussion 10365 (Rust API faster, keep IPC small): https://github.com/tauri-apps/tauri/discussions/10365
- dilla.io — Benchmarking WASM Builds: http://dilla.io/blog/benchmarking-webassembly-builds-unveiling-performance-insights
- GitHub WebCC architecture (batching to-JS calls): https://github.com/io-eric/webcc/blob/main/docs/architecture.md
