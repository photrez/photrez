# Frame-pacing baseline (large-document interaction)

Durable record of a measured claim that previously existed only as an oral
assertion. Produced by `scripts/frame-pacing-live.mjs`.

**Every table below names the binary that produced it.** Do not mix them.

## What is being claimed

"Large-document interaction free of 33ms-class jank" is a **frame-pacing**
claim: it is about the interval between *presented* frames, not about how long
a render callback takes. The in-app instrument times only the synchronous render
callback (`apps/desktop/src/renderer/scheduler.ts:28-32`), which excludes GPU
present, texture upload, the Tauri IPC hop and vsync — so it cannot observe a
dropped frame. It is additionally gated behind `import.meta.env.DEV`
(`apps/desktop/src/components/editor/shell/BottomStatusBar.tsx:51`), so it does
not exist in a release build at all.

A jsdom or vitest substitute is not valid: jsdom has no compositor and no GPU,
and its `requestAnimationFrame` is timer-driven, so a jsdom frame time is a
fiction. These numbers come from the real WebView2 window.

## Status: FAILS — reduced 4.5-6x since the repair, still failing

**The requirement still fails at HEAD `8767ed0`.** It failed harder before: the
repair that landed as `8767ed0` brought the cold max frame from 16,417-19,301 ms
down to 2,716.8-3,216.9 ms, and the per-frame budget is still exceeded by 32-41
frames per run. See **"Status at `8767ed0`"** for the post-repair tables and for
why the dropped-frame count went *up* while the worst frame went *down*.

The tables immediately below are the **pre-repair** measurement at `34dad0cd` and
are kept unedited as the baseline the repair is judged against. Steady-state
percentiles are and were perfect throughout (median 16.7 ms, p95 16.8-16.9 ms —
exactly vsync); every failure in this document is a stall, not a pacing drift.

**The cause is attributed: it is the code delta, not the build route.** See
"Attribution" below — this was established by rebuilding the older tree through
the identical toolchain, route and harness.

### `34dad0cd` (pre-repair) — **FAILS**

Binary: exe mtime **`2026-10-06 05:23:55`**, built via
`bun run tauri build --no-bundle` (frontend embedded; verified at runtime by
serving `http://tauri.localhost/`). HEAD commit date `2026-10-06 03:58:18`, so
the binary is newer than the commit it builds.

Interaction: real brush strokes as CDP `Input.dispatchMouseEvent`, which enters
the browser input pipeline and yields genuine
`pointerdown`/`pointermove`/`pointerup` for the production dispatcher. 12 strokes
x 40 moves. Brush size 20 (app default). "cold" = document created seconds
earlier, 0 warm-up strokes; "warm" = 3 warm-up strokes first.

| Document | Mode | n | frames | median | p95 | p99 | **max** | **frames >= 33ms** |
|---|---|---|---|---|---|---|---|---|
| 4096 x 4096 | cold | 3 | 584 / 685 / 661 | 16.7 | 16.8 | 33.3-33.4 | **16167 / 17518 / 16768** | **7 / 9 / 7** |
| 4096 x 4096 | warm | 3 | 978 / 1432 / 1263 | 16.7 | 16.9 | 33.4-700.1 | **1417 / 19634 / 13484** | **26 / 17 / 16** |
| 2048 x 2048 | warm | 3 | 1301 / 1286 / 1315 | 16.7 | 16.8-16.9 | 166.7-550.1 | **14334 / 16367 / 16467** | **20 / 21 / 29** |

All nine runs paint-proven by a composited-pixel diff, document size verified
from the real signals, canvas centre hit-testing to `CANVAS`.

## Attribution: the code delta, not the build route

Two variables had changed at once between the only clean binary and the failing
one — the code, and the build route. They have now been separated.

| | HEAD `34dad0cd` | `b62c60d8` rebuilt |
|---|---|---|
| exe mtime | `2026-10-06 05:23:55` | `2026-10-06 06:00:46` |
| build route | `bun run tauri build --no-bundle` | **same** |
| toolchain | `stable-x86_64-pc-windows-gnu`, rustc 1.95.0 | **same** |
| harness / protocol / machine / session | identical | **same** |
| source commit | `34dad0cd` (2026-10-06) | `b62c60d8` (2026-09-30) |
| cold 4096 max | 16167 / 17518 / 16768 ms | **17.4 / 17.2 / 17.2 ms** |
| cold 4096 frames >= 33ms | 7 / 9 / 7 | **0 / 0 / 0** |
| cold wall | 27-33 s | **9.4 s** |

`b62c60d8` is the newest commit at or before the mtime of the original clean
2026-09-30 binary (built 19:17:34; newest commit at that instant was
`b62c60d8`, 18:25:46). It was built in a detached `git worktree` on C: with
`CARGO_TARGET_DIR` on C:, so D: was never written to.

**Conclusion: the stall is a regression introduced by the code delta
`b62c60d8..34dad0cd`** — 56 commits, first bad at #23. The build route is
exonerated: the older tree built through the same route on the same machine in the
same session is clean, 3-for-3.

> **A note on that clean column, because it is easy to misread.** An entry in the
> change history for this effort reports a PASS of "9 runs, 0 frames >= 33 ms" —
> which is exactly the `b62c60d8` column above, and exactly this binary. That
> result came from a **2026-09-30 binary**, and it is **not** a result for any
> current HEAD: the harness was re-run at `8767ed0` and measures 32-41 dropped
> frames per run. The history entry is append-only and is left as written; this
> note is the correction, and it is here because a reader arriving at the history
> first would otherwise take the PASS as current.

## Bisect: `9dc10677` introduces the stall

**`9dc10677` — `fix(editor): fence the brush on Rust store presence, not graph ownership` (2026-10-02)** is the first bad commit. It is the **23rd of 56** commits in `b62c60d8..34dad0cd`; its parent is `99afe618`. (Position verified with `git rev-list --reverse b62c60d8..34dad0cd` — 56 entries, `9dc10677` at 1-based index 23. An earlier revision of this document said 24th; that was an off-by-one and is corrected here.)

### How the bisect was run

- **Shared warm target.** One detached `git worktree` on C: (`C:\fp\wt`) with `CARGO_TARGET_DIR` inside it, so every step reused one release cache: **822 s** for the first cold build, **150-222 s** per subsequent step (only `photrez-core` and `photrez-desktop` recompile). D: was never a build target.
- **Predicate.** Cold 4096², 12 strokes x 40 moves, **n=1 per step**. BAD if any frame ≥ 33 ms exceeded **200 ms**. The threshold is not delicate: a good commit measures **17-19 ms** and a bad one **15,900-19,600 ms** — a ~1000x gap, versus the 1.7-2.1x run-to-run spread on this box.
- **Blank-page trap.** The harness was run **from the main tree** (`D:\Project\image-studio\scripts\frame-pacing-live.mjs`, committed at `8552ff5`) against each worktree's binary, so every step used the guard-bearing harness regardless of what the checked-out commit contained. Every step's log records `target=http://tauri.localhost/` plus `paintProven: true` — no step measured a blank page or a no-op stroke.
- **Route.** Identical to the tables above: `bun run tauri build --no-bundle`, then the harness.

Steps tested, in order: `d69c387c` BAD 19,700.9 · `f9b71b2a` GOOD 18.6 · `cadfd7a4` GOOD 17.2 · `cce459ef` BAD 15,933.9 · `9dc10677` BAD 16,717.4 · `99afe618` GOOD 17.2. No commit was skipped. (`cadfd7a4` is a deps-only commit and measured GOOD, as a control it must.)

**Two measurement failures were caught and corrected rather than reported as results.** The first `git bisect run` reported "first bad commit" in under two minutes with **zero driver log lines** — the build rewrites `apps/desktop/src-tauri/Cargo.toml` line endings, so every internal `git checkout` failed and each step returned a default-bad code without running. Fixed with `core.autocrlf=false` plus a restore, then re-run with each step verified by its own `RESULT` line. Separately, `99afe618` was marked bad once by bookkeeping error; its measured value was GOOD and it is reported as such.

### Boundary confirmation (n=3 each, same route and harness)

| Commit | max frame (ms), n=3 | frames ≥ 33 ms | paint-proven |
|---|---|---|---|
| `99afe618` (parent) | **17.7 / 17.3 / 17.2** | 0 / 0 / 0 | yes |
| `9dc10677` (first bad) | **16917.3 / 16417.4 / 18050.7** | 8 / 8 / 10 | yes |

The boundary is exactly where the bisect predicted it.

### What the commit changed (mechanism, as first hypothesised)

`9dc10677` changes no pixel code. It replaces the brush's entry gate in
`apps/desktop/src/components/editor/useBrushOverlay.ts`:

- **Before** (`useBrushOverlay.ts` at `9dc10677^`), Gate A refused the stroke outright on any facade-owned layer: `if (isFacadeOwnedLayer(activeId)) { showToast(...); return; }`. On a newly created document the layer is facade-owned, so **the stroke was refused and the entire downstream commit never ran** — which is why the "good" commits measure 17 ms.
- **After** (`useBrushOverlay.ts:717-730` and `:1131-1143`), that refusal is replaced by a per-stroke store-presence fence, `strokeBlockedByRust` (`useBrushOverlay.ts:174-187`), which blocks only on a positive "Rust does not hold this layer" and otherwise lets the stroke through.

So the commit does not *add* work; it **un-gates a path that was previously unreachable in this scenario**, and that path is expensive on a large document. As first written down, from the source:

1. First stroke finds no Rust storage, so it seeds the store with a **full-layer readback**: `surface.readRect(0, 0, w, h)` then `rust_pixels_init` (`useBrushOverlay.ts:217-220`) — 67 MB of pixels for 4096².
2. The commit syncs the post-stroke surface back into the model via `surface.toImageBitmap()` + `engine.setLayerImageBitmap` (`useBrushOverlay.ts:276-285`).
3. `setLayerImageBitmap` unconditionally calls `pushModelToRust` (`apps/desktop/src/engine/document.ts:1455`), which `JSON.stringify`s the whole model and calls `restore_snapshot` (`document.ts:1767-1792`).

**Step 1 is correct and is the whole story. Steps 2 and 3 are wrong**, and are kept above rather than deleted because being wrong about them is the more useful record: a plausible `file:line` chain is not a measurement.

### The mechanism, now MEASURED (was a hypothesis)

The three steps above were a hypothesis read off the source. They have since been
apportioned by measurement, and the answer is **none of them in the form stated**.
Step 1 is real and dominant; steps 2 and 3 are not the cost.

A phase-tagged rAF sampler plus a per-stage in-WebView measurement at 4096²
attribute the 16 s as follows. The seed is the whole layer, so it crosses the IPC
as **67,108,864 bytes**.

| stage | measured | note |
|---|---|---|
| `surface.readRect(0,0,w,h)` → `getImageData` | **35.7 ms** | the 67 MB readback itself is cheap |
| `JSON.stringify` of the seed payload | **15,060.7 ms** | **94.6% of the whole stall** |
| `createImageBitmap` | 21.7 ms | |
| `pushModelToRust` / whole-model replay | **< 1 ms** | model JSON is metadata only — `document.ts:1771-1774` strips `imageBitmap` / `baseImageBitmap` before it is sent, so there is nothing large to serialise |
| `rust_pixels_write_region` round trip, per stroke | 589–880 ms | see "Per-stroke wire" below |

**The cost is Tauri v2's IPC JSON serialisation, not the readback and not the
model replay.** 67 MB of pixels became a **268,435,768-character** JSON string.
This was confirmed independently of the app: running Tauri's exact replacer over
the same `Uint8Array` outside the app reproduced it at **17,499 ms** for
268,435,520 characters. The mechanism is `process-ipc-message-fn.js` — the replacer
that tauri-runtime injects into every webview — expanding any `Uint8Array` argument
with `Array.from(val)`, one JSON array element per byte.

The hypothesis in steps 2 and 3 was wrong in a way worth recording: the
whole-model replay was assumed to be significant and measures **under 1 ms**,
because the model payload is stripped of its bitmaps before it is sent. The
falsifiable part of the original hypothesis — that `9dc10677` un-gates an
expensive path — was correct, and it was correct for a reason the hypothesis
misidentified.

### Per-stroke wire, and why both directions had to change

The first stroke's seed was the largest single item, but the per-stroke commit
path had the same defect and was left running on the byte-array wire:

| | request | response |
|---|---|---|
| byte-array wire (before) | 5,414,721 chars | 27,264,034 chars (13 tiles x before+after) |
| base64 wire (now) | 3,588,864 chars | 9,089,352 chars |

Measured in-app at the 3254x208 dirty rect the harness strokes: request
serialisation **589–880 ms**, base64 alternative **156 ms**. So the seed alone
would have left ~0.6–0.9 s of main-thread block on **every committed stroke**,
plus ~27 MB of JSON parsed per stroke on the response — at any document size.

### Two attempts, one of which failed informatively

**Attempt 1 — raw binary body (INERT, and worth keeping as a record).** The seed
was posted as the payload itself rather than as an argument field, on the theory
that Tauri then skips JSON entirely and uses `application/octet-stream`
(`process-ipc-message-fn.js:9-12`, `ipc-protocol.js` `sendIpcMessage`). It did not
work, and it is worth recording *why*, because the reasoning was sound and the
premise about which code path runs was not:

- Tauri only skips JSON when the **whole IPC payload** is a typed array, **and**
  only on the custom-protocol path. This app runs the **postMessage fallback**,
  where the object handed to the serialiser is the *envelope*
  `{cmd, callback, error, options, payload}` — so a typed-array payload nested
  inside it is expanded straight back into a number array.
- Verified three ways on the shipped binary: the bundle did contain the change
  (`layer-width` present in `dist/assets/index-*.js`), the seed still produced
  268,435,786 JSON characters at **20,036.4 ms**, and Rust rejected the call with
  `rust_pixels_init expects a raw binary body` — the body arrived as `InvokeBody::Json`.
- The `fetch` wrapper for `ipc:` recorded **zero** calls, confirming the
  custom-protocol path is never taken here.
- Measured result: cold max frame **24,267.7 ms** — *worse* than the
  16,417–19,301 ms it was meant to fix, with 27 dropped frames.

**The mock-fidelity gap this exposed.** The test double replicated
`process-ipc-message-fn.js` faithfully — the serialiser was transcribed correctly,
byte for byte — and every assertion was green. What no test covered was the
**branch selection**: *which* code path calls the serialiser. A faithful mock of
the wrong function proves nothing. That is precisely the failure mode AGENTS.md's
mock-fidelity rule exists to catch, and it is recorded here because the same gap
produced two wrong "fixed" claims before it was found by measuring the shipped
binary instead of the mock.

**Attempt 2 — base64 (shipped, `8767ed0`).** Correct on **both** IPC branches,
because a base64 string crosses as one string and `JSON.stringify` copies it in a
single pass. Applied to every pixel-bytes field on the path, not just the seed —
`rust_pixels_init`, `rust_pixels_write_region` (request and response),
`rust_pixels_resize_layer`, `apply_tile_patch` (request and response), and every
tile list those commands return. Leaving one command on the byte-array wire would
have handed the next caller the identical stall with nothing in the code to warn
them.

## Where the stall is (localised, pre-repair binary `34dad0cd`)

A phase-tagged continuous rAF sampler ran across the whole session — from before
the document dialog opened to after the last stroke — attributing every frame to
the phase active when it began. Times are ms from sampler start. **These figures
are from the pre-repair binary; at `8767ed0` the stroke-1 frame is
2,716.8–3,216.9 ms, not 17,567 ms.**

| Phase | frames | total | **max frame** | frames >= 33ms |
|---|---|---|---|---|
| boot | 1 | 16.6 | 16.6 | 0 |
| open New Document dialog | 57 | 983 | 50.1 | 1 |
| type width/height | 64 | 1067 | 17.1 | 0 |
| **click Create -> document ready** | 32 | 700 | **133.3** | 3 |
| select brush | 40 | 667 | 17.2 | 0 |
| **stroke 1** | 45 | **17567** | **16700.5** | 2 |
| strokes 2-8 | ~50 each | ~800-900 each | 16.8-17.2 | 0 |
| stroke 9 / 10 / 11 / 12 | 52/105/81/108 | 1633/3634/2417/2834 | 783/1150/633/1050 | 1/3/3/1 |

Marks: dialog opened at 1; size typed at 992; Create clicked at 2049; document
ready at 2753 (**704 ms from Create click to canvas visible**); brush selected at
3437; **stroke 1 spanned 3446 -> 21032 (17586 ms)**; strokes 2-8 then ran at a
steady ~800 ms each; strokes 9-12 degraded again.

**Established:** the multi-second stall is **entirely inside the first brush
stroke** on a newly created large document. It is **not** app launch and **not**
document creation — creating a 4096 x 4096 document took 704 ms and its worst
frame was 133 ms. A **second, smaller** degradation appears from stroke 9 onward
(633-1150 ms), growing with stroke count.

The introducing commit is named (`9dc10677`, see "Bisect" above), measured at the
boundary with n=3 on each side. Its mechanism **has since been apportioned by
measurement** — see "The mechanism, now MEASURED" — but this paragraph and the
phase table above were taken at `34dad0cd`, the pre-repair binary. The stroke-1
figure of 17,567 ms there is the pre-base64 stall; at `8767ed0` the equivalent
cold frame is 2,716.8–3,216.9 ms.

The stroke-9-onward degradation (633–1,150 ms, growing with stroke count) was
**not** separately apportioned. It is consistent with the per-stroke
`rust_pixels_write_region` cost, which is measured at 344–387 ms per call at
4096², but consistency is not attribution and it is not claimed as such.

## What is NOT claimed

- **Commit count is UNKNOWN.** History is owned by the Rust side, so the JS
  `CommandHistory` (`getUndoCount`, `getHistoryStack`) stays at 0 / length 1
  while real commits land — it is not a commit oracle. The real oracle is the
  read-only Rust probe `rust_pixels_history_depth(doc_id)`, which needs a
  document id; that id is reachable only through the dev handle
  `window.__photrezEditor`, absent in a release build because
  `shouldExposeEditorDebugHandle()` returns false for `MODE=production`
  (`apps/desktop/src/components/editor/shell/EditorContext.tsx:82`). The script
  wires the Rust probe when the dev handle is present and reports UNKNOWN
  otherwise. No commit count is guessed.
- **Not a worst case.** At fit zoom a 4096 x 4096 document rasterises to an
  885 x 664 CSS canvas, so it is large in model terms but small in rasterised
  area. Zoom-to-100 %, full-document redraw and many-layer documents are not
  covered.
- **Not a shipping gate.** See below.
- **The introducing commit is `9dc10677`**, measured at the boundary 3-for-3 on
  each side. Its mechanism has since been apportioned by measurement — see "The
  mechanism, now MEASURED" — but that apportionment was taken at `8767ed0`, on a
  different binary from the bisect tables above, and the bisect itself remains a
  one-configuration measurement.
- **REDUCED, not fixed.** See "Status at `8767ed0`" above. The repair landed; the
  requirement still fails, and the dropped-frame count got *worse*, not better.
- **The bisect measured one configuration.** Cold 4096², brush, default flags,
  release profile, `photrez.tileCommit` left at its default ON. It says nothing
  about other document sizes, the eraser, or a flag set that skips the Rust path.

## Status at `8767ed0` — REDUCED, not fixed

This section supersedes the "not fixed" verdict an earlier revision of this
document carried. The base64 change has landed (`8767ed0 perf(paint): carry pixel
bytes as base64 across the process boundary`).

Binary: exe mtime **2026-10-06 17:53:46**, built via
`bun run tauri build --no-bundle`. Same route, toolchain, machine, session and
harness as every table above.

| Document | Mode | n | **max frame (ms)** | **frames >= 33 ms** | paint-proven |
|---|---|---|---|---|---|
| 4096 x 4096 | cold | 3 | **3216.9 / 2716.8 / 2716.8** | **35 / 34 / 32** | yes |
| 4096 x 4096 | warm | 3 | **4150.2 / 4216.9 / 4083.6** | **30 / 41 / 41** | yes |
| 2048 x 2048 | warm | 3 | **616.8 / 499.8 / 550.2** | **27 / 24 / 26** | yes |

Against `9dc10677` cold at **16,417–19,301 ms / 7–8 dropped** — and against the
inert raw-body attempt at **24,267.7 ms / 27 dropped**.

### What improved, and what did not

**The max frame fell 4.5–6x.** The seed's `JSON.stringify` went from 15,060.7 ms
to **65 ms** (268,435,768 chars → 89,478,567), and its base64 encode was
separately brought from **2,412 ms to 530 ms** in the shipped WebView (the
`Array.from` wrapper around `String.fromCharCode.apply` was materialising a
24,576-element JS array per chunk, 2,731 times per layer). The 2048² runs land at
0.5–0.6 s, which is where the fix was predicted to put them.

**The dropped-frame count got worse: 7–8 → 32–35.** This is not a regression in
the fix; it is what the fix exposes. The baseline hid its cost inside **one**
enormous frame. That cost is now spread across 12 strokes at roughly 385 ms each.
The requirement counts dropped frames, so on the requirement's own terms the
repair made the number worse while making the worst frame much better.

**Do not read "under 3.5 seconds" as "the requirement is met."** It is not met.
The requirement is a per-frame bound, and 32-41 frames still violate it per run.

One more correction to an earlier reading of these numbers. A first pass concluded
that the dropped-frame count was unchanged because 14-16 had been measured with
the seed fixed alone; that comparison was against the wrong baseline. Against
`9dc10677` cold (7-8) the count is 32-35, and against the same tree with only the
seed fixed (14-16) it is also up. Both comparisons point the same way, so the
statement above stands either way: the repair redistributed the cost, it did not
remove it.

### Where the remaining time is

Per-stage, measured in the real WebView at 4096²:

| stage | ms |
|---|---|
| seed base64 encode | 530 |
| seed invoke round trip | 1,198 |
| `write_region` encode | 93–105 |
| **`write_region` round trip** | **344–387** |
| `write_region` response decode | 28–72 |
| `snapshot_layer` parse (89 MB) | 39 |

12 strokes x ~385 ms ≈ 4.6 s, which accounts for the warm frame.

The residual was then attacked directly, by varying the payload size rather than
by wrapping the transport. Three commands with deliberately lopsided payloads
separate the two halves, because a pixel round trip has a request cost and a
response cost and each can be loaded alone:

| probe | request | response | round trip |
|---|---|---|---|
| no-op command, no document | ~0 | ~0 | **0.6 ms** (fixed dispatch overhead) |
| `rust_pixels_init` on 4096² | 67 MB | none | **964 ms** (request side alone) |
| `rust_pixels_snapshot_layer` on 4096² | ~0 | 89.5 MB | **3,203 ms** (response side alone) |
| `rust_pixels_write_region` 16x16 | 1.4 KB | 699 KB | **19 ms** |

Two results follow, and they are the useful part:

1. **The response side is roughly 3x the request side per byte**, and the webview's
   own share of it is small: `JSON.parse` of that same 89.5 MB body measured
   **34 ms**, i.e. **1.1%** of the 3,203 ms. So the cost is not the host parsing
   its own response — it is everything between the webview handing the request over
   and the result arriving as usable bytes.
2. **Fixed overhead is negligible** (0.6 ms), so none of this is dispatch latency
   or promise plumbing.

A size sweep then confirms the response is the driver, over a 256x range:

| region | request chars | response chars | round trip (median of 3) |
|---|---|---|---|
| 16x16 | 1,368 | 699,056 | 21 ms |
| 64x64 | 21,848 | 699,056 | 20 ms |
| 256x256 | 349,528 | 699,056 | 24 ms |
| 512x512 | 1,398,104 | 2,796,224 | 105 ms |
| 3254x208 | 3,609,772 | 9,087,728 | 344 ms |
| 1024x1024 | 5,592,408 | 11,184,896 | 389 ms |
| 2048x1024 | 11,184,812 | 22,369,792 | 815 ms |
| 4096x4096 | 89,478,488 | 178,958,336 | 12,074 ms |

The first three rows vary the request 255x while the response is held constant, and
the cost does not move — so request size is not the driver. Cost then tracks
response size linearly: ~0.036 ms per 1,000 response characters, predicting within
17% of measured across the seven points up to 2048x1024. At 3254x208 (the harness's
dirty rect) the fit attributes 329 ms of the measured 344 ms to the response.

The 4096x4096 row is the exception: the fit predicts 6,520 ms against 12,074 ms
measured, so the largest case is **superlinear** in a way the linear fit does not
explain and this document does not claim to.

**Still not resolved.** The split of that ~385 ms between Tauri's transport copy
and Rust's own write is **not measured**. Two things blocked it, both recorded
rather than worked around:

- The obvious monkey-patches are on the wrong objects. `window.ipc.postMessage`
  and `window.fetch` saw **zero** calls; the runtime's own `invoke` source shows
  it calls `window.__TAURI_INTERNALS__.ipc(...)`, and both `ipc` and `invoke` are
  **non-writable and non-configurable** on that object, so they cannot be wrapped
  or redefined from the page at all.
- A sampling CPU profile does not separate them either: it attributes the time to
  `(program)` — native frames with no JS symbol — which is exactly the region the
  two candidate explanations both live in.

**So the residual is the RESPONSE, established by three independent routes**
(lopsided-payload probes, a request-vs-response sweep, and a linear fit). What is
still missing is the split *inside* it, and that is now a narrower question than it
was: not "is it the request or the response", but "how much of the 385 ms is
carrying 9 MB of JSON across the process boundary versus how much is Rust building
and encoding those 26 tiles". That decides whether the follow-up is a binary
channel or a change in the write path, and it is **not decided here**.

### Cold: the extra ~1 s is still unattributed

Cold max is 2,716.8-3,216.9 ms and the seed accounts for 530 ms of encode plus
964-1,198 ms of round trip, i.e. ~1.5-1.7 s. **The remaining ~1-1.5 s of the cold
frame is not attributed.** The probes above ran against an already-seeded layer;
the cold frame also contains document creation and the first-stroke composite, and
those were not separately timed in the same run. The pre-repair phase table in
this document puts `click Create -> document ready` at 704 ms, which is the right
order of magnitude for most of the gap, but that figure is from `34dad0cd` and is
**not** a measurement of the cold frame at `8767ed0`. It is flagged rather than
reused."

### The commit-path `toImageBitmap()` is MEASURED, and it is NOT a priority

`PaintTileSurface.toImageBitmap()` (`apps/desktop/src/lib/paint/paintTileSurface.ts`)
was the largest cost in the commit path with no entry in any per-stage table:
`useBrushOverlay.ts` calls it on every committed stroke, and at 4096² it
materialises a 67,108,864-byte surface. It is now timed behind the same
`localStorage.photrez.c4Audit` flag as the commit audit above, so both land in one
console stream for the same stroke.

Measured in the real release app, cold, 12 strokes after 3 warmup strokes,
**one app relaunch per run** (`--cold` requires no open document):

| surface | run | calls | min | median | max | surface bytes |
|---|---|---|---|---|---|---|
| 4096² | 1 | 9 | 0 ms | **0.1 ms** | 0.2 ms | 67,108,864 |
| 4096² | 2 | 9 | 0.1 ms | **0.1 ms** | 0.4 ms | 67,108,864 |
| 4096² | 3 | 9 | 0 ms | **0.1 ms** | 0.3 ms | 67,108,864 |
| 2048² | 1 | 14 | 0 ms | **0.1 ms** | 0.2 ms | 16,777,216 |
| 2048² | 2 | 14 | 0.1 ms | **0.1 ms** | 0.2 ms | 16,777,216 |
| 2048² | 3 | 14 | 0 ms | **0.1 ms** | 0.3 ms | 16,777,216 |

**This does not mean the 67 MB are free, and the naive reading of it is wrong.**
Two measured facts change how the number must be interpreted:

1. `performance.now()` is clamped to **100 µs** in this webview (measured live:
   the smallest non-zero consecutive delta is exactly 100 µs), so a reported 0 ms
   means "under one clock tick", not "no work".
2. `createImageBitmap()` resolves a **lazy handle**. Chromium resolves the promise
   when the handle exists, not when the pixels have moved. Timing the same
   surface's first real consumer separates the two:

| surface | handle resolution | first CPU read | repeat CPU read | first GPU upload |
|---|---|---|---|---|
| 2048² (16.8 MB) | 0.1 ms | 73.9 ms | 47.7 ms | **0.2 ms** |
| 4096² (67.1 MB) | 0.2 ms | 326.6 ms | 155.6 ms | **0.4 ms** |

A GPU upload of 67 MB costing **0.4 ms** against a CPU read of **326.6 ms** is
aliasing, not copying: the bitmap shares the surface's backing store. The commit
path does not force the bytes — it stores the handle
(`engine.setLayerImageBitmap`), `createSnapshot` keeps it **by reference**
(`apps/desktop/src/engine/snapshot.ts:40`), `pushModelToRust` strips
`imageBitmap` before serialising (`document.ts:1772`), and the renderer is fed
**dirty-rect tiles** via `queueOrUploadTiles`, not the full bitmap. So the
67 MB are never duplicated per stroke on this path.

**Decision, stated explicitly.** The rule was: over ~385 ms, `toImageBitmap()`
takes top priority and outranks shrinking the `write_region` response. Measured
at **0.1 ms median / 0.4 ms worst case at 4096², 2,000x under the threshold**,
it is not top priority. **The response stays top priority.**

The one caveat worth recording: the deferred bytes are real and someone eventually
pays them. A CPU readback of the committed surface is 326.6 ms at 4096². That cost
is currently unattributed to any specific user-visible operation, and the paths
that would force it (undo restore, save, CPU-side export) are the place to look if
a per-frame budget is ever missed there. It is **not** on the stroke commit path,
so it does not change the prioritisation above.

Corroboration from the same runs: the new harness capture reports a median
`responseBytes` of 7,340,032 at 4096² and 4,194,304 at 2048². Carried through the
~1.33x base64 expansion and this document's own ~0.036 ms/1,000-response-chars fit,
that predicts **351 ms** at 4096² against the 344-387 ms measured above — an
independent path to the existing number, from a harness that never measured the
response round trip directly.

## Re-running

The app must already be running; the script attaches, it does not launch.

```sh
# 1. release profile - the CLI is REQUIRED, see the trap below
bun run build
bun run tauri build --no-bundle
export WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9225
./target/release/photrez-desktop.exe &

# 2. measure (cold asserts no document is open, so relaunch between runs)
FP_CDP_PORT=9225 FP_PROFILE=release \
  bun scripts/frame-pacing-live.mjs --cold --warmup 0 \
    --doc-w 4096 --doc-h 4096 --strokes 12 --moves 40 --label cold-1

# 3. steady state
FP_CDP_PORT=9225 FP_PROFILE=release \
  bun scripts/frame-pacing-live.mjs --warmup 3 \
    --doc-w 4096 --doc-h 4096 --strokes 12 --moves 40 --label warm-1
```

This workspace builds to the **repo-root** `target/` directory, not to
`apps/desktop/src-tauri/target/`.

### Environment prerequisites (both bite silently)

1. **The CLI is required.** `cargo build --release -p photrez-desktop` compiles
   cleanly and the exe **runs**, but with **no embedded frontend**: the webview
   falls back to the devUrl (`http://localhost:1420`), so the window comes up
   with no editor chrome. A harness that does not check will "measure" a blank
   page. This happened here: 9 runs reported a UI-shaped failure against an app
   with no UI. `scripts/frame-pacing-live.mjs` now refuses to run unless the app
   serves `http://tauri.localhost/` **and** its chrome is present.
2. **Toolchain is directory-scoped.** `D:\Project\image-studio` carries a rustup
   directory override to `stable-x86_64-pc-windows-gnu`. A checkout without that
   override falls back to the msvc default host and fails to link, because no MSVC
   linker is installed. Reproducing these numbers from another directory requires
   `rustup override set stable-x86_64-pc-windows-gnu --path <dir>`.

## Not a hard gate

Frame timing is machine-dependent and must not fail CI on a loaded machine. The
default action of the script is to **report**. Pass `--max-dropped <n>` to opt
into a threshold. Run-to-run spread on identical settled code on a loaded box
has been observed at 1.7-2.1x for throughput numbers, so any future threshold
needs a margin. The failure recorded above is reproduced 3-for-3 and is far
outside that spread.

## Why the original wording is not falsifiable

The clause specifies no document size, no interaction, no percentile, no run
count and no build profile. Two runs of the same code differed (0 vs 8 dropped
frames) purely on whether the document had just been opened. It also presumes a
60 Hz panel, where every frame longer than 16.7 ms is already a dropped frame —
so "33 ms-class" really means "dropped at least one frame", a coarse binary
rather than a pacing bound.

Every one of those knobs is an explicit defaulted parameter of
`scripts/frame-pacing-live.mjs`, so a run is reproducible and self-describing.
Wording the requirement as "no frame >= 33 ms across N runs of M strokes on a
4096 x 4096 release build, p99 <= X ms" would make it falsifiable — and, run
today, it fails.

## Machine used for the numbers above

Windows, session 1, 1536 x 816 viewport, 60 Hz panel, 4-6 GB free RAM,
~257 processes. These figures describe that machine only.