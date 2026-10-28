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

## Status: FAILS, ONE ORDER OF MAGNITUDE SMALLER SINCE THE RAW-TRANSPORT BINARY (2026-10-08)

**The requirement still fails.** It failed hardest before any repair (cold max
frame 16,417-19,301 ms at `9dc10677`); the base64 repair (`8767ed0`) brought that
down to 2,716.8-3,216.9 ms but left 32-41 dropped frames per run; the
re-measurement at the bottom of this document ("Re-measurement on the raw-byte
transport binary (2026-10-08)") now reads warm max 50.1-83.3 ms with 11-15
dropped frames. See **"Status at `8767ed0`"** for the post-repair tables and for
why the dropped-frame count went *up* while the worst frame went *down* - the
shape of the failure has changed twice since, and **each dated section below
stands as the record of its own commit.**

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
| byte-array wire (original) | 5,414,721 chars | 27,264,034 chars (13 tiles x before+after) |
| base64 wire | 3,588,864 chars | 9,089,352 chars (13 tiles x before+after) |
| base64 wire, post-image only (current) | 3,588,864 chars | **4,544,687 chars** (13 tiles) |

**The last row is a later change, measured in the shipped app.** The reply no
longer carries a pre-image: the write had already appended one to Rust's own pixel
history, and undo returns those bytes from there, so nothing could read the copy in
the reply. That is **4,545,641 characters removed per stroke at this rect**, which
is why the row above is now historical rather than current.

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

**CORRECTION — the corroboration this section previously carried was
invalid, and the number it produced was a coincidence.** It read a median
`responseBytes` of 7,340,032 at 4096² and 4,194,304 at 2048², pushed those through
the ~1.33x base64 expansion and this document's own ~0.036 ms/1,000-chars fit, and
reported the resulting 351 ms as "an independent path to the existing number".

It was not independent, and it was not the response size. `responseBytes` summed
**only the post-image tiles** (`useBrushOverlay.ts`, the `[paint-commit]` audit
line), while the reply was still carrying a pre-image of the same size. So the
figure was **half** the response, and the fit was calibrated on *whole*-response
character counts from the size sweep above. Applying one to the other happened to
land near the measured 344-387 ms, which is what made it look like agreement.

Two consequences, both stated rather than smoothed over:

1. **That paragraph was not evidence.** The 344-387 ms attribution rests on the
   size sweep and the lopsided-payload probes alone, which still stand.
2. **A metric named for the response measured part of it.** Renamed to
   `postImageBytes`, and it is now structurally incapable of reporting a
   whole-reply size.

The reply's real size at the 3254x208 rect, measured in the shipped app rather
than inferred: **4,544,687 characters** post-image, against 9,090,328 for the
before+after reply it replaced.

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

## Re-measurement on the raw-byte transport binary (2026-10-08)

The status sections above are the record as of `8767ed0`/`1089f5bb`. On top of that tree,
seven paint-path commits landed (`c05d0513`..`b563a7d3`) together with the raw-byte region
write transport (`apps/desktop/src/lib/protocol/paintRegionIpc.ts`,
`apps/desktop/src-tauri/src/paint_parity_raw.rs` - the commit immediately before the one that
adds this section), and the same harness was re-run against the release exe built from that
tree. **These are the newest
numbers; every older row above stands as written for its own commit.**

| Document | Mode | n | frames | median | p99 | **max (ms)** | **frames >= 33 ms** | paint-proven |
|---|---|---|---|---|---|---|---|---|
| 4096 x 4096 | warm | 3 | 593 / 593 / 592 | 16.7 | 33.4-66.6 | **50.1 / 83.3 / 66.8** | **11 / 15 / 11** | yes |
| 4096 x 4096 | cold | 1 | 630 | 16.7 | 33.3 | **766.5** | **11** | yes |

Against the recorded post-base64 rows (warm max **4,083.6-4,216.9 ms** / **30-41** dropped;
cold max **2,716.8-3,216.9 ms** / **32-35** dropped), the worst frame is 40-80x smaller and
the dropped count ~3x smaller. Stated plainly, so it is not over-read:

- **The delta is CUMULATIVE.** The binary carries the seven landed paint commits PLUS the
transport change set. Nothing here isolates the transport's share of the frame-time win; the
transport's isolated win is measured separately at command level (median 122.6 -> 79.4 ms on
a 3254x208 rect, both commands alternating in one session against the real release exe).
- **The cold row is n=1** (the warm row is n=3), and its 766.5 ms max frame is the
  first-stroke seed (full-layer readback + `rust_pixels_init`) - the same attribution the
  earlier cold rows carry.
- **THE REQUIREMENT STILL FAILS.** `frames >= 33 ms` is not zero in any row. What changed is
  the SHAPE: the multi-second single stall is gone; what remains is 11-15 dropped frames
  across ~600 frames, worst frame tens of ms warm and 766.5 ms cold.
- Reproduction: warm = `--warmup 3 --doc-w 4096 --doc-h 4096 --strokes 12 --moves 40`; cold
  = the same with `--cold --warmup 0` on a fresh launch. Labels
  `release-4096-rawtransport` / `-w2` / `-w3` / `-cold`.

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

## Native production-seam measurement: `measure-frames` (2026-10-19)

These rows are a different binary and a different display path from every table
above. The tables above measure the shipped v0.1.0 WebView2 runtime through
`scripts/frame-pacing-live.mjs` (browser pointer injection plus Tauri IPC). This
section measures the native GPUI canvas seam in `crates/photrez`; that seam does
not exist in v0.1.0. Do not compare a row in one section with a row in the other.

**Binary:** `crates/photrez/src/bin/measure-frames` (`photrez` 0.2.0, dev profile
with `opt-level = 2` for the gpui crates and the app crate; `gpui-kit` 0.7.3 with
the `profiler` feature; `gpui-pre` 0.3.9).

**Machine:** Windows x86_64, 8 logical CPUs, AMD Radeon(TM) Graphics (integrated),
1024x768 window. The adapter and machine strings are repeated in every JSON row;
the rows below describe this machine only.

Command:

```sh
cd crates/photrez
cargo run --bin measure-frames -- --strokes 12 --moves 40
```

**What is measured:** document-update-to-present through the production
`render::canvas_bridge` seam. One run is one process: open the window, wait for
its first frame, optionally warm up, upload the full document, then for each move
paint a disc into the retained RGBA trail, clone it (the seam consumes and swaps
its input), create a new `RenderImage`, notify, and wait for the profiler's
present event. `upload_dirty_ms` is `null`: the seam has no partial-upload path,
so reporting a dirty number would be fabrication. There is no pointer injection:
the native shell has no production pointer path yet, so this is not
pointer-to-brush.

`RenderImage` is a snapshot. GPUI's sprite atlas keys every image by its unique
id; a new image per update is retained unless retired. The bench evicts the
previous texture with `Window::drop_image` at the start of the next render.
Without that eviction a 4096 run stops mid-run on a 64 MiB allocation; the
eviction is a lifecycle cost of the snapshot path, not a measurement shortcut.

### 4096x4096, warm (n=3)

Window 1024x768, 12 strokes x 40 moves, brush 32 document px, 3 warm-up moves.

| run | upload_full_ms | frame_p95_ms | frames >= 33 ms | update_p95_ms | update_max_ms | paint-proven |
|---|---|---|---|---|---|---|
| 1 | 70.94 | 82.77 | 479 / 480 | 82.45 | 87.59 | yes (1.32%) |
| 2 | 82.02 | 83.99 | 479 / 480 | 83.72 | 104.85 | yes (1.32%) |
| 3 | 70.78 | 85.04 | 479 / 480 | 84.82 | 94.39 | yes (1.32%) |

### 4096x4096, cold (n=3)

Same interaction, no warm-up moves. Each run is a fresh process/window.

| run | upload_full_ms | frame_p95_ms | frames >= 33 ms | update_p95_ms | update_max_ms | paint-proven |
|---|---|---|---|---|---|---|
| 1 | 76.83 | 87.03 | 479 / 480 | 86.66 | 91.89 | yes (1.32%) |
| 2 | 76.08 | 87.57 | 479 / 480 | 87.08 | 112.43 | yes (1.32%) |
| 3 | 75.04 | 88.77 | 479 / 480 | 88.46 | 97.81 | yes (1.32%) |

### 800x600 demo document, warm (n=3)

The real demo document, same 12 x 40 interaction. This is the product's default
document size, not the 4096 harness size.

| run | upload_full_ms | frame_p95_ms | frames >= 33 ms | update_p95_ms | update_max_ms | paint-proven |
|---|---|---|---|---|---|---|
| 1 | 4.96 | 18.41 | 0 / 480 | 16.43 | 18.29 | yes (10.50%) |
| 2 | 7.14 | 18.40 | 0 / 480 | 16.56 | 19.17 | yes (10.50%) |
| 3 | 7.06 | 18.47 | 0 / 480 | 16.23 | 18.34 | yes (10.50%) |

### Fault-injection check

`--red upload` injects a 40 ms delay immediately before the full upload. The run
must report the delay and exit non-zero; otherwise the instrument cannot see a
known-bad frame.

| check | upload_full_ms | reported verdict | exit |
|---|---|---|---|
| 40 ms injected before the full upload | 57.02 | `upload_full_ms 57.02 > 16.7` | 1 |

### Reading

1. **4096x4096 fails by a wide margin.** The full-document upload alone is
   70.78-82.02 ms, and the present interval is 82.77-85.04 ms p95; 479 of 480
   frames are at or above 33 ms. The cost is the 64 MiB snapshot created and
   uploaded per move. This is the silent cliff the display contract warns about:
   a full-canvas upload per frame is not viable at this size. A partial-upload
   path is now the required next step, not an optional optimization.
2. **800x600 is close but not green.** The demos have zero frames at or above
   33 ms and a 16.23-16.56 ms update p95, but the present-interval p95 is
   18.40-18.47 ms, about 1.5 ms over the 16.9 ms target. The clean-boundary rows
   above measured 17.2-17.7 ms maxima on the WebView2 path, so the target sits at
   the edge of this machine's present cadence. The number is recorded as a tight
   margin, not rounded into a pass.
3. **Atlas retention is real.** A new `RenderImage` per update is a new id; the
   sprite atlas retains each one until `Window::drop_image` removes it. The
   bench's eviction keeps a 480-frame run inside the machine's memory; a
   continuous canvas path must do the same.
4. **Scope limits.** Full upload only (`upload_dirty_ms` is `null`), no pointer
   injection, and the adapter/machine strings are printed in every JSON row. The
   numbers describe this machine only.

### Re-running the native rows

```sh
cd crates/photrez
cargo run --bin measure-frames -- --strokes 12 --moves 40
cargo run --bin measure-frames -- --mode cold --strokes 12 --moves 40
cargo run --bin measure-frames -- --demo --strokes 12 --moves 40
cargo run --bin measure-frames -- --demo --strokes 1 --moves 1 --warmup 0 --red upload
```

The last command is expected to exit 1. Each run prints one JSON row; `--out
PATH` also writes it to a file.

## Native production-seam measurement: dirty-rect upload with a patch overlay (2026-10-20)

**The display seam changed since the section above, so the rows below are not
comparable with it.** The section above measured a full-document upload per
move. The seam now uploads only the dirty rectangle: the document is held as a
**512 px base-tile grid**, and a live **patch overlay** carries the dirty
rectangle until it is folded back into the base tiles.

**Binary:** `crates/photrez/src/bin/measure-frames` (`photrez` 0.2.0, dev profile
with `opt-level = 2` for the gpui crates and the app crate; `gpui-kit` 0.7.3 with
the `profiler` feature; `gpui-pre` 0.3.9). The bench reports
`dirty_strategy: base_tiles_plus_patch_overlay`.

**Machine:** Windows x86_64, 60 Hz panel, AMD Radeon(TM) Graphics (integrated),
1024x768 window. 4096x4096 document unless the demo document is named; brush
radius 32 document px, so the dirty rectangle is 65x65. Warm n=5, cold n=3,
demo warm n=3.

### What the seam does

- The document is uploaded as a grid of 512 px base tiles. Base tiles are
  immutable images; an unchanged tile keeps its identity and its atlas entry.
- A dirty region is uploaded as its own **patch** image, carrying only the
  dirty rectangle.
- Patches are **folded into the touched base tiles every 8 dense moves**, so a
  long stroke does not accumulate an unbounded patch list.
- A region update is refused while patches are pending, and the texture
  accessor returns the base tiles together with the live patches.
- The bench takes **12 isolated dirty samples** for the dirty-upload metric;
  each sample is committed right after it is sampled, so exactly one patch is
  in the scene while it is measured. The dense phase then reports the update
  and commit statistics.

### Targets

| metric | target | role |
|---|---|---|
| `upload_dirty_ms` | <= 16.7 ms | the dirty-upload budget |
| `frame_p95_ms` | <= 16.9 ms | do not regress the present-interval p95 |
| `frames_ge_33ms` | 0 | no dropped-frame class |
| `upload_full_ms` | record only | per-frame full upload is not a product operation |

The full-upload figure is retained as a record, not as a verdict; the fault
injection switches are retained so both instruments stay falsifiable.

### 4096x4096, warm (n=5)

12 strokes x 40 moves, 3 warm-up moves. 60 patch commits per run in every row
below (480 dense moves folded every 8 moves).

| run | upload_full_ms | upload_dirty_ms | frame_p95_ms | frames >= 33 ms | update_p95_ms | update_max_ms | commit_p95_ms | commit_max_ms | paint-proven |
|---|---|---|---|---|---|---|---|---|---|
| 1 | 111.44 | 16.27 | 17.59 | 0 | 16.63 | 17.97 | 16.92 | 18.06 | yes (1.3166%) |
| 2 | 140.88 | 15.68 | 17.62 | 1 | 16.77 | 18.22 | 17.49 | 18.20 | yes (1.3166%) |
| 3 | 116.75 | 15.73 | 17.77 | 0 | 16.86 | 18.45 | 17.28 | 17.84 | yes (1.3166%) |
| 4 | 103.07 | 17.07 | 17.86 | 4 | 16.99 | 18.13 | 17.35 | 19.37 | yes (1.3166%) |
| 5 | 116.19 | 16.22 | 17.76 | 1 | 16.64 | 17.97 | 17.37 | 18.37 | yes (1.3166%) |

The first three runs of this table are also reported as the warm n=3 sample:
dirty 16.27 / 15.68 / 15.73 ms, frame p95 17.59 / 17.62 / 17.77 ms, frames
>= 33 ms 0 / 1 / 0.

### 4096x4096, cold (n=3)

Same interaction, no warm-up moves. Each run is a fresh process/window.

| run | upload_full_ms | upload_dirty_ms | frame_p95_ms | frames >= 33 ms | update_p95_ms | update_max_ms | commit_p95_ms | commit_max_ms | paint-proven |
|---|---|---|---|---|---|---|---|---|---|
| 1 | 152.46 | 15.98 | 17.65 | 1 | 16.47 | 17.38 | 16.74 | 17.58 | yes (1.3166%) |
| 2 | 130.60 | 16.14 | 17.92 | 1 | 16.98 | 18.69 | 17.64 | 18.16 | yes (1.3166%) |
| 3 | 121.88 | 15.93 | 17.92 | 2 | 16.99 | 18.36 | 18.06 | 18.93 | yes (1.3166%) |

### 800x600 demo document, warm (n=3)

The real demo document, same 12 x 40 interaction. This is the product's default
document size, not the 4096 harness size.

| run | upload_full_ms | upload_dirty_ms | frame_p95_ms | frames >= 33 ms | update_p95_ms | update_max_ms | commit_p95_ms | commit_max_ms | paint-proven |
|---|---|---|---|---|---|---|---|---|---|
| 1 | 6.17 | 16.80 | 17.60 | 2 | 16.94 | 17.91 | 16.83 | 17.77 | yes (10.6812%) |
| 2 | 12.35 | 16.10 | 17.57 | 1 | 16.61 | 18.21 | 16.80 | 17.03 | yes (10.6812%) |
| 3 | 10.25 | 16.30 | 17.89 | 3 | 16.84 | 18.45 | 17.19 | 17.56 | yes (10.6812%) |

### Fault-injection check

Both switches must report the injected delay and exit non-zero; otherwise the
instrument cannot see a known-bad frame.

| check | measured | reported verdict | exit |
|---|---|---|---|
| `--red dirty` (delay before the dirty upload) | `upload_dirty_ms` 47.67 | dirty upload over the 16.7 ms budget | 1 |
| `--red upload` (delay before the full upload) | `upload_full_ms` 144.88 | full upload over the 16.7 ms budget | 1 |

### Comparison with the other upload strategies measured on this machine

Same binary family, same machine, same interaction. The two tiled rows without
a patch overlay are earlier iterations of this work and are listed for
comparison only.

| strategy | warm `upload_dirty_ms` | warm `upload_full_ms` | warm `frame_p95_ms` | warm frames >= 33 ms |
|---|---|---|---|---|
| single full image, no tiles (2026-10-19 section) | - | 70.78-82.02 | 82.77-85.04 | 479 / 480 |
| 256 px tiles, no patch overlay | 17.83 / 15.44 / 14.00 | 136.61 / 136.80 / 139.04 | 18.50 / 18.58 / 18.80 | 4 / 0 / 2 |
| 512 px tiles, no patch overlay | 17.21 / 17.19 / 17.84 | 154.95 / 122.18 / 146.29 | 17.63 / 17.59 / 17.99 | 0 / 1 / 0 |
| **512 px tiles + patch overlay (this section)** | **16.27 / 15.68 / 15.73 / 17.07 / 16.22** | 111.44 / 140.88 / 116.75 / 103.07 / 116.19 | 17.59 / 17.62 / 17.77 / 17.86 / 17.76 | 0 / 1 / 0 / 4 / 1 |

### Reading

1. **The dirty upload is inside the budget most of the time, not always.**
   Against the 16.7 ms budget: 4 of 5 warm runs pass (16.27 / 15.68 / 15.73 /
   16.22 ms; the 17.07 ms run misses), 3 of 3 cold runs pass (15.93-16.14 ms),
   and 2 of 3 demo runs pass (16.80 ms misses by 0.10 ms). This is not a
   universal pass and is not reported as one.
2. **The present-interval p95 is still above its target.** 17.57-17.92 ms
   everywhere, against a 16.9 ms target. That range sits inside the deviation
   band (17.46-18.33 ms) that was already recorded for this machine, so it is
   carried as a **recorded deviation**, not as a green pass.
3. **Dropped frames are not zero.** Frames >= 33 ms: warm 0 / 1 / 0 / 4 / 1,
   cold 1 / 1 / 2, demo 2 / 1 / 3. The measured update max (17.38-18.45 ms) and
   commit max (17.58-19.37 ms) both stay below 19.37 ms, so these stalls are
   **not attributable to the measured canvas update/commit path**; they are
   environment/compositor residual. The target of zero is not met, and the
   residual is not claimed as fixed.
4. **The full upload is record-only, and the tiled opening upload is a real
   regression.** 103.07-152.46 ms at 4096 (demo 6.17-12.35 ms) against
   70.78-82.02 ms for the single-image path measured on 2026-10-19. The
   per-frame full upload remains a design constraint, and the tiled opening
   upload costs more than the single image did.
5. **Known limitation.** A 1 px document-edge tile can map below one device
   pixel; it is skipped rather than squeezed. This is a documented limitation
   of the tile grid, not a claim about the seam's correctness.
6. **Paint-prove holds in every run.** 1.3166 % changed pixels at 4096 and
   10.6812 % in the demo.

### Re-running the native rows

```sh
cd crates/photrez
cargo run --bin measure-frames -- --strokes 12 --moves 40
cargo run --bin measure-frames -- --mode cold --strokes 12 --moves 40
cargo run --bin measure-frames -- --demo --strokes 12 --moves 40
cargo run --bin measure-frames -- --demo --strokes 1 --moves 1 --warmup 0 --red upload
cargo run --bin measure-frames -- --demo --strokes 1 --moves 1 --warmup 0 --red dirty
```

The last two commands are expected to exit 1. Each run prints one JSON row;
`--out PATH` also writes it to a file.

### Idle-repaint control (2026-10-20)

The control keeps the same window and document and repaints the same scene 480
times per run, with **no canvas mutation**. Three warm-up repaints precede the
measured phase. A before/after GDI capture must be **unchanged**; this is the
inverse of the normal paint-prove and proves the run really is idle.

| run | 4096 p95 (ms) | 4096 frames >= 33 ms | demo p95 (ms) | demo frames >= 33 ms | canvas changed |
|---|---|---|---|---|---|
| 1 | 17.53 | 1 | 17.77 | 2 | 0.0% |
| 2 | 17.80 | 4 | 17.51 | 0 | 0.0% |
| 3 | 17.46 | 0 | 18.10 | 0 | 0.0% |

Every run reported 480/480 presents and `control_unchanged: true`.

- The **application-present idle floor** is 17.46-18.10 ms p95, above the
  16.9 ms target. PresentMon's **displayed** p95 for the 4096 control is
  16.93 ms (one displayed 33 ms stall), so the target is 0.03 ms over on the
  display, not below a 17.5 ms machine floor. The target is not met.
- The **33 ms class is not zero at idle** (4096 1/4/0; demo 2/0/0). The 4-frame
  run reproduces with no canvas update at all, so those stalls are not
  attributable to the canvas update/commit path.
- The control does **not** measure `upload_dirty_ms`; the 17.07 ms warm and
  16.80 ms demo dirty outliers remain recorded misses.
- `paint_proven` is false in control rows by design (nothing changed); the
  validity check is `control_unchanged`, not a paint change.

```sh
cd crates/photrez
cargo run --bin measure-frames -- --control --strokes 12 --moves 40
cargo run --bin measure-frames -- --demo --control --strokes 12 --moves 40
```

### Application-present vs displayed interval (2026-10-20, frame-callback wait)

The runs below replace the measurement loop's 2 ms sleep poll with GPUI's
`Window::on_next_frame` plus a one-shot wake. The poll was inside the measured
loop; it did not change a recorded present timestamp, but it could delay the
next notify. PresentMon 2.6 captured the same runs.

`frame_p95_ms` in the JSON is the **application present interval**: it matches
PresentMon's `MsBetweenPresents` exactly (17.61 ms). It is not the **displayed
interval** (`MsBetweenDisplayChange`). On this machine the displayed interval
is lower:

| run | app p95 (ms) | display p95 (ms) | app >= 33 ms | display >= 33 ms |
|---|---|---|---|---|
| 4096 control, frame-callback wait, n=3 | 17.60 / 17.46 / 17.71 | 16.93 (one run captured) | 0 / 0 / 1 | 1 (33.29) |
| 4096 warm normal, frame-callback wait, n=3 | 17.61 / 17.68 / 17.65 | 16.92 (one run captured) | 0 / 0 / 0 | 1 stroke (33.33; startup/full-upload intervals excluded) |

- The **displayed** p95 is 16.92-16.93 ms: 0.02-0.03 ms over the 16.9 ms target,
  and one displayed 33 ms stall remains. This is not a pass, and the target is
  not redefined here. The **application** p95 is 17.61 ms; the earlier
  "17.46-18.10 ms idle floor" is that application interval, not the displayed
  one.
- The frame-callback wait removes the **application** 33 ms stalls: control
  0/0/1 and warm 0/0/0, against the earlier 1/4/0 and 0/1/0/4/1. The remaining
  displayed stall is separate from the application present interval.
- The wait has a **measurement cost**: cold `upload_dirty_ms` is
  16.79 / 17.06 / 17.04 ms, failing all three against <= 16.7 ms, versus
  15.98 / 16.14 / 15.93 ms with the old poll. Warm is 16.65 / 16.53 / 16.72 ms
  (2/3) and demo 16.65 / 16.63 / 16.99 ms (2/3). This is a loop-timing
  regression, not a canvas-path regression.
- The production edit path is still not wired to the patch overlay; the tiled
  opening upload remains 103-152 ms. No green claim.

## Sampling wait, displayed gate, and opening experiments (2026-10-20)

This section separates three clocks that the earlier rows mixed. The product
gate is the **displayed** interval from PresentMon's
`MsBetweenDisplayChange` column. The GPUI application present interval
(`MsBetweenPresents`) is diagnostic. The per-update sample uses the same
frame-callback wait for the dense loop, control and compose, but a bounded
2 ms sample wait for the isolated full-document and dirty samples, so the
measurement does not pay a one-frame callback cost it is trying to measure.

Method: one process per run; 4096x4096 and 800x600 demo; 12 strokes x 40
moves; 32 document px brush (65x65 dirty rect); 60 commits; GDI paint-prove;
PresentMon 2.6 CSV with `--presentmon-skip-ms 4000` so the startup and
full-upload rows (including sub-100 ms present rows) are excluded by
`TimeInMs`, not only by a duration filter. The displayed target remains
p95 <= 16.9 ms and zero displayed frames >= 33 ms. The dirty target remains
`upload_dirty_ms` <= 16.7 ms.

The capture form is two commands: start PresentMon writing a CSV while the
bench runs, then pass that CSV back to the same JSON row for the gate.

```powershell
# terminal 1: capture displayed intervals
presentmon.exe --process_name measure-frames.exe --output_file target\g6-presentmon-warm.csv --timed 20 --terminate_after_timed
# terminal 2: run the product seam
cargo run -p photrez --bin measure-frames -- --size 4096 --mode warm
# gate the captured intervals; skip the startup/full-upload prefix
cargo run -p photrez --bin measure-frames -- --size 4096 --mode warm --presentmon-csv target\g6-presentmon-warm.csv --presentmon-skip-ms 4000
```

### Default 512 px tiles, 8-move commits, split wait

| mode | `upload_dirty_ms` (3 runs) | application p95 (3 runs) | application >= 33 ms | `upload_full_ms` (3 runs) | paint-proven |
|---|---|---|---|---|---|
| cold | 17.30 / 16.53 / 17.24 | 17.53 / 17.68 / 17.72 | 0 / 0 / 0 | 146.14 / 127.50 / 166.36 | yes |
| warm | 16.68 / 16.34 / 16.73 | 17.62 / 17.59 / 17.69 | 0 / 0 / 0 | 119.57 / 123.99 / 113.95 | yes |
| demo | 16.96 / 16.66 / 17.86 | 17.51 / 17.61 / 17.53 | 0 / 0 / 0 | 4.47 / 15.31 / 15.17 | yes |

Every row reports 480/480 presents and `paint_proven: true`. The isolated
dirty population is now published per sample (`dirty_samples_ms`,
`dirty_max_index`), not only as a maximum. The worst cold sample in the
follow-up was 17.55 ms at index 5; the cold max still misses the 16.7 ms
target in 2 of 3 runs.

Displayed replay of the warm 4096 capture: p95 **16.92 ms**, **1** displayed
frame >= 33 ms, 517 samples after 54 skipped opening rows. The same capture's
application interval is p95 17.60 ms with 0 application frames >= 33 ms.
The control replay is displayed p95 16.93 ms with one 33.29 ms stall. The
displayed 16.9 ms target is 0.02 ms over and the zero-33 target is missed.
This is the official deviation; neither target is redefined.

### Default-off experiments

| experiment | cold dirty (3) | warm dirty (3) | application p95 (6) | application >= 33 ms (cold 3 + warm 3) | commit p95 (6) | result |
|---|---|---|---|---|---|---|
| 1024 px tiles | 16.41 / 16.03 / 15.73 | 16.24 / 16.33 / 16.44 | 17.70-17.94 | 0 / 0 / 0 + 0 / 0 / 0 | 18.75-19.18 | dirty 6/6, but commit/application worse; off |
| 16-move commits | 17.36 / 16.88 / 16.44 | 16.46 / 17.75 / 16.54 | 17.51-17.81 | 0 / 0 / 0 + 0 / 12 / 0 | 18.10-18.82 | one warm run returns 12 application 33 ms frames; off |
| patch union | 16.47 / 17.09 / 15.97 | 16.88 / 16.55 / 16.88 | 17.80-18.27 | 0 / 0 / 0 + 0 / 0 / 0 | 17.67-18.43 | 60 updates instead of 480, but application p95 worse; off |
| progressive first quarter | first 77.39 / 73.14 / 83.63 | first 90.20 / 74.71 / 93.22 | 17.53-17.81 | 0 / 0 / 0 + 11 / 14 / 0 | n/a | 2/3 cold first paint <= 82 ms, 1/3 warm; full still 105.76-128.89, total 180.69-213.83; off |

The 1024 px tile experiment is the only one that makes the dirty target
universal (6/6), but it moves the cost into commits and the application
interval; it is not a free win. The 16-move commit and progressive runs
reintroduce application 33 ms frames, so they are rejected. Patch union
reduces the number of uploads by 8x but raises the application p95; it is
rejected. The progressive first paint sometimes lands under 82 ms, but the
full document still takes more than 100 ms and the total is worse than the
synchronous 512 px opening; the default opening remains synchronous.

### Opening and production wiring

The full 512 px tiled upload is 113.95-166.36 ms at 4096, versus the
single-image 70.78-82.02 ms measured in the 2026-10-19 section. The
progressive probe does not close that gap. The native shell still has no
pointer/paint path: `app.rs` owns a one-shot `CanvasImage`, `input.rs` is a
pointer-policy placeholder, and `views/workspace.rs` receives
`Option<&CanvasImage>` for display. Native app edit wiring is therefore not
claimed; it needs a separate input/paint specification.

Defaults remain 512 px base tiles and an 8-move commit interval. No
production default, ownership surface, or target changed. With the defaults,
the displayed and dirty targets remain unmet; not green.

## Native synthetic input-to-commit smoke (2026-10-25)

This dated result updates the earlier **Opening and production wiring** note:
a synthetic event now traverses the real native App through the production
canvas hitbox using `Window::dispatch_event`. It measures a software
input-to-commit boundary, not physical input, displayed-frame pacing,
smoothness, or display latency.

> Note (workspace consolidation): the command form below is the pre-consolidation two-workspace form (`--manifest-path crates/photrez/Cargo.toml`); today the equivalent is `cargo run --locked -p photrez --features input-measurement --bin measure-input -- --samples 1`. The numbers were also produced by a build where `photrez-core` compiled at `opt-level = 0`; they are not comparable with post-consolidation measurements.

Command: `cargo run --locked --manifest-path crates/photrez/Cargo.toml --features input-measurement --bin measure-input -- --samples 1`.
The build took 51.04 s; the executable exited 0 with status `ok` and claim
`synthetic_input_to_commit_only`.

| Receipt field | Result |
|---|---|
| Fixture; sample | `bounded-raster.ptz`; 1 |
| Document; layer | `parity-doc`; `raster-1` |
| Epoch; version | 0 -> 1; 0 -> 1 |
| Canonical bytes changed; refresh succeeded | true; true |
| Dispatch to commit | 0.4760 ms |
| Handler entry to commit; pointer-up entry to commit | 0.4221 ms; 0.2349 ms |
| App render after commit | 0.3812 ms |
| Render identity | `WindowId(1v1)` |
| Draw start/end offsets from commit; draw duration | 0.3233 / 10.3497 ms; 10.0264 ms |
| Commit to submission completion | 22.3092 ms |
| Physical display latency | null (`not_measured`) |

The sample uses a Windows x86_64 machine with 8 CPUs and a development build.
The commit timestamp follows successful commit; render evidence requires the
matching window identity and successful refresh. This single small-fixture
sample is not a 4096 x 4096 benchmark and proves no performance threshold.
The prior displayed-frame verdict and targets above are unchanged.

## Single-sample live recheck (2026-10-25)

This is a second separately recorded run; the earlier 0.4760 ms sample remains
unchanged. This is a one-sample software-boundary smoke, not a performance or
frame-pacing benchmark.

> Note (workspace consolidation): pre-consolidation two-workspace command form; today `cargo run --locked -p photrez --features input-measurement --bin measure-input -- --samples 1`. Measured on a build where `photrez-core` compiled at `opt-level = 0` — non-comparable with post-consolidation numbers.

Command: `cargo run --locked --manifest-path crates/photrez/Cargo.toml --features input-measurement --bin measure-input -- --samples 1`.
The build took 28.60 s; the executable exited 0 with JSON status `ok` and claim
`synthetic_input_to_commit_only`.

| Receipt field | Result |
|---|---|
| Canonical bytes changed; epoch; version | true; 0 -> 1; 0 -> 1 |
| Refresh succeeded; render identity | true; `WindowId(1v1)` |
| Dispatch to commit | 0.7951 ms |
| Handler entry to commit; pointer-up to commit | 0.7041 ms; 0.3934 ms |
| App render after commit | 0.2966 ms |
| Draw start/end offsets from commit; draw duration | 0.2395 / 10.4705 ms; 10.231 ms |
| Commit to submission completion | 23.045 ms |
| Physical display latency | null (`not_measured`) |

The CLI accepts only `--samples 1`; advertised counts 2-8 are rejected.
`present_end < draw_end` is also rejected. The two guard cases were observed
RED (2 failures) before correction; the focused suite then passed 12/12, and
native formatting plus the feature-bin check exited 0.

The driver loads a controlled fixture after `App::new`; this does not prove the
startup `--open` route. Review found no blocking issue for this demonstrated
one-sample path. The final root and native feature-on/off gate remains pending.
No hardware latency, smoothness, benchmark, or performance-threshold claim is
made.

## G8 vehicle extension and early 4096 smoke (2026-10-26)

The `measure-input` vehicle now accepts `--samples`, `--strokes`, `--moves`,
`--canvas`, `--brush-sizes`, and `--base`. A canvas larger than 2 is generated
at runtime as a `.ptz` archive with a stored-zlib PNG layer; the bounded 2x1
fixture remains the default. Each stroke records dispatch/handler/up/commit/
render/draw timing, stage submission completion, changed pixels and 256 px
tiles, and a pixel digest. Aggregates report p50/p95/max per brush size. RSS is
not measured in-process (`rss_status: external_sampling_required`). The output
reports `profile: dev|release`.

Metric mapping: `pointer_to_brush_ms` is `dispatch_to_commit_ms` (down dispatch
to successful commit), not Q6's pointer-event-to-first-stroke-pixel. The native
app commits the stroke at pointer-up and has no live first-pixel commit, so the
Q6 first-pixel definition remains unimplemented. The harness also dispatches
down/moves/up as one burst before waiting for a single commit render, so it
cannot measure a per-move first-pixel present even if a preview existed. A live
preview and a per-move render/present wait are required before the Q6 metric can
be measured.

Smoke results, all development profile and single-run:

| Workload | Dispatch to commit | App render after commit | Changed px / tiles |
|---|---:|---:|---:|
| bounded 2x1, brush 2, 1x1 | 0.5081 ms | 0.6257 ms | 1 / 1 |
| generated 64x64 blank, brush 8, 1x2 | 1.3434 ms | 1.1616 ms | 419 / 1 |
| generated 512x512 opaque, brush 40, 2x4 | 61.70 / 62.89 ms | 91.60 / 95.01 ms | 19,532 / 2 per stroke |
| generated 4096x4096 blank, brush 800, 1x1 | **2413.89 ms** | **3161.03 ms** | 876,272 / 28 |

The 4096 result also measured handler-entry-to-commit 2413.78 ms,
up-to-commit 2117.39 ms, draw 106.07 ms, and commit-to-submission 3281.48 ms.
It is far above the 33.3 ms target and is recorded as an early failing
measurement, not the canonical G8 run. The canonical 4096x4096, brush 800/40,
12x40, blank/opaque, cold/warm, n>=3 run has not been executed. The plan's
failure branch applies: the failing metric is recorded, and the conditional
optimization spec (dirty-rect/tiled commit and display, or a Tier C GPU
decision) is the next investigation. No brush math, default, or target changed.
The development profile is not a performance claim; a release-profile run and
the Q6 first-pixel decision are still open. RSS remains externally sampled.

## 4096² release baseline (2026-10-26)

The release build required the environment fixes recorded in ADR-0016: MSYS2
`windres` on `PATH` and `RC`, plus `GPUI_FXC_PATH` for `fxc.exe`. The exact build
(pre-consolidation two-workspace command form; today `cargo build --locked
--release -p photrez --features input-measurement --bin measure-input`; the
numbers were produced by a build where `photrez-core` compiled at
`opt-level = 0` and are not comparable with post-consolidation measurements):

```text
PATH=/c/msys64/mingw64/bin:$PATH \
RC='C:\msys64\mingw64\bin\windres.exe' \
GPUI_FXC_PATH='C:\Users\Qolbi\photrez-tools\winsdk\10.0.28000.0\x64\fxc.exe' \
cargo build --locked --manifest-path crates/photrez/Cargo.toml \
  --features input-measurement --bin measure-input --release
```

It finished in 5m26s; `target/release/measure-input.exe` is 64.7 MB. The run:

```text
target/release/measure-input.exe --samples 1 --strokes 1 --moves 1 \
  --canvas 4096 --brush-sizes 800 --base blank
```

| Release receipt field | Result |
|---|---:|
| Dispatch to commit | 308.30 ms |
| Handler entry to commit | 308.25 ms |
| Pointer-up to commit | 252.37 ms |
| App render after commit | 216.91 ms |
| Draw duration | 54.53 ms |
| Commit to submission completion | 282.73 ms |
| Changed pixels / tiles | 876,272 / 28 |
| Digest | `8a612fb286f19f05` |

The dev-profile comparison is 2413.89 ms dispatch/3161.03 ms render; release is
about 7.8x faster on commit and 14.6x faster on render, with an identical pixel
digest. It is still about 9.3x over the 33.3 ms commit bar and 6.5x over it on
render, and the 54.5 ms draw alone exceeds the bar. This is the full-commit
path, not Q6 first-pixel; the dirty patch/live preview work and the canonical
12x40 cold/warm n>=3 run remain open. No target or default changed.

### Single-layer dirty display patch (2026-10-26)

The brush commit now consumes the core `after` `TilePatch` set and updates the
existing display snapshot through `CanvasImage::push_patch` and
`commit_patches` for a single-layer document. Multi-layer documents, or a
patch failure, fall back to the full composite. Same release command/workload:

| Metric | Full composite | Dirty patch |
|---|---:|---:|
| Dispatch to commit | 308.30 ms | 317.05 ms |
| App render after commit | 216.91 ms | **16.56 ms** |
| Draw duration | 54.53 ms | **17.10 ms** |
| Commit to submission completion | 282.73 ms | **49.09 ms** |
| Digest | `8a612fb286f19f05` | `8a612fb286f19f05` |

The render and draw portions now sit under the 33.3 ms bar; the core commit
(~317 ms) is unchanged and remains the dominant cost. The displayed first
pixel still follows that commit, so the full-commit path cannot satisfy Q6
first-pixel. A live preview plus a per-move render/present wait is still
required, and the core commit optimization is still open. The dirty path is
deliberately single-layer only; multi-layer keeps the full composite. No target
or default changed.

The frozen reference confirms the Q6 shape: its brush overlay is seeded with the
layer, draws dabs into a 2D canvas per move (the composite/RAF path), and
`commitBrushStroke` runs at pointer-up. Q6 first-pixel is therefore the overlay
pixel, not the core commit; the core commit is the separate `brush_commit`
record. The native app has no live overlay, so its first-pixel path is still
missing.

## Live preview 12x40 release (2026-10-26)

The native single-layer live preview now pushes an exact `raster_shadow` crop
through `CanvasImage::push_patch` on pointer-down and each move; the final commit
applies the exact `after` tiles. The Q6 first-pixel is the first preview present
after pointer-down, measured per stroke.

Command (release):

```text
target/release/measure-input.exe --samples 1 --strokes 12 --moves 40 \
  --canvas 4096 --brush-sizes 800,40 --base blank
```

| Brush | first-pixel p50/p95/max | preview push p50/p95 | commit p50/p95 | render p50/p95 | changed px/tiles |
|---|---:|---:|---:|---:|---:|
| 800 | 68.85 / **85.83** / 91.19 ms | 61.73 / 71.77 ms | 657.01 / 743.47 ms | 29.28 / 35.72 ms | 10,515,264 / 336 |
| 40 | 15.63 / **19.66** / 24.54 ms | 11.42 / 13.45 ms | 202.34 / 210.19 ms | 3.92 / 7.79 ms | 709,007 / 120 |

The 40 brush passes the 33.3 ms first-pixel target; the 800 brush fails and its
preview push is the bottleneck. Commit metrics are record-only. This is a
reduced one-sample early measurement, not the green canonical G8: cold/warm,
opaque, n>=3 remain open. The 800 optimization spec is
`.slim/deepwork/g8-800-preview-spec.md`; no target, brush math, or default
changed.

## Display-scale preview 12x40 release (2026-10-26)

R1 display-scale preview plus a settings-keyed tip cache. The preview raster is
`min(viewport zoom, cap/crop)`; the tip is resampled to the preview stamp, and
the pointer-up commit discards every preview before the exact tiles land. The
earlier 800 number was dominated by rebuilding the 800 px tip on every
pointer-down (~40 ms) and re-rasterizing the exact full-resolution crop.

Canonical command (release), cap 256:

```text
PHOTREZ_PREVIEW_MAX_EDGE=256 target/release/measure-input.exe --samples 3 \
  --strokes 12 --moves 40 --canvas 4096 --brush-sizes 800,40 --base opaque
```

Per-sample p95 (s1 / s2 / s3):

| Base | Brush | first-pixel | preview push | commit p95 range |
|---|---|---:|---:|---:|
| opaque | 800 | 12.28 / 9.09 / 8.30 ms | 3.68 / 4.79 / 3.71 ms | 585-653 ms |
| opaque | 40 | 4.74 / 5.46 / 5.70 ms | 0.10 / 0.12 / 0.09 ms | 193-207 ms |
| blank | 800 | 9.05 / 8.85 / 9.05 ms | 2.72 / 3.73 / 3.50 ms | 440-612 ms |
| blank | 40 | 4.23 / 4.75 / 4.01 ms | 0.09 / 0.11 / 0.09 ms | 177-188 ms |

All first-pixel/preview numbers are under the 33.3 ms target for both brushes,
both bases, cold and warm samples. The worst cold first stroke was 22.27 ms
(sample 1, 800 px, opaque) and remains under target. Commit stays record-only;
the bounded-crop commit is the next wave.

Cap A/B (opaque, n=3, worst per-sample p95): 128 -> 800 12.53 ms / 40 5.40 ms;
256 -> 12.28 / 5.70 ms; 384 -> 10.38 / 7.48 ms. All caps pass and are nearly
indistinguishable because the 4096 fit zoom is 0.169, so the display scale (not
the cap) bounds the preview. 256 is kept: it matches the frozen overlay's
`PREVIEW_MAX_SIZE`, and it leaves headroom above fit zoom for the same raster.

The tip cache is a behavioural fix, not a target change: `build_brush_tip_data`
skips the diameter-space diagnostic upscales the native rasterizer never reads,
and `BrushTipCache` reuses one tip per (size, hardness, colour). A regression
test pins the scaled tip against the old top-left-corner sampling.

## Bounded crop commit 12x40 release (2026-10-26)

R2(2): `compute_commit` now premultiplies and rasters only the tile-aligned
bounding box of the dab batch, then shifts the extracted tiles back into
document space. `commit_pixels_with_phases` reports the crop and per-phase
timings, and the measurement receipt carries them into the JSON. Two in-core
parity tests assert byte-identical before/after tiles against the full-pass
reference: interior dabs at two tiles, and a wide stroke whose brush (400 px)
is larger than its tip data (128 px).

Same command as above, opaque n=3, cap 256. Pointer-up -> commit (record-only):

| Brush | before p50/p95 | after p50/p95 | premultiply p50 | raster p50 | patch p50 | apply p50 | last crop |
|---|---:|---:|---:|---:|---:|---:|---|
| 800 | 237.1 / 267.3 ms | 166.3-172.4 / 188.4-200.9 ms | 30.1-30.9 ms | 20.0-20.7 ms | 81.5-86.3 ms | 21.5-22.0 ms | 4096x1024 |
| 40 | 139.8 / 170.3 ms | 51.9-54.9 / 90.9-97.2 ms | 7.8-9.6 ms | 10.7-12.4 ms | 24.8-25.4 ms | 4.9-5.3 ms | 4096x256 |

Phase columns are per-sample p50 ranges across the three runs. The G8
first-pixel/preview gate is unchanged and still green (worst p95 12.54 ms
first-pixel, 4.27 ms preview push). Patch generation (unpremul of the touched
tile bytes) is now the largest core phase; the exact accumulator is not needed
for the gate.

Digest caveat: repeated invocations of the **same** binary produce different
stroke bytes (sample 1 brush 800 changed 1,790,036 vs 1,792,552 px with
different digests) because the synthetic pointer path maps through live window
bounds. Cross-run digests are not a correctness proof; exactness rests on the
two in-core full-pass parity tests.

## Settled-tree final G8 (2026-10-26)

Final release rerun after the G6/G7/G9/G10 changes (custom pointer surface,
cursor context policy, corrected `G`/`Shift+G`, bracket keys, selected-layer
projection). Command as above, cap 256, opaque/blank, n=3:

| Base | Brush | first-pixel p95 s1/s2/s3 | preview push p95 s1/s2/s3 | commit p95 s1/s2/s3 |
|---|---|---:|---:|---:|
| opaque | 800 | 12.25 / 12.87 / 9.73 ms | 4.47 / 5.48 / 4.33 ms | 630 / 611 / 610 ms |
| opaque | 40 | 7.77 / 4.97 / 7.15 ms | 0.12 / 0.11 / 0.13 ms | 127 / 117 / 130 ms |
| blank | 800 | 7.59 / 8.94 / 10.22 ms | 2.58 / 3.51 / 4.61 ms | 358 / 598 / 610 ms |
| blank | 40 | 5.84 / 5.88 / 6.21 ms | 0.10 / 0.11 / 0.11 ms | 106 / 110 / 112 ms |

Worst first-pixel p95 across the settled matrix: **12.87 ms**; worst preview
push: **5.48 ms**. Both are under the 33.3 ms target for both brushes, both
bases, cold and warm samples. Commit remains record-only; the bounded crop is
4096x1024 for 800 and 4096x256 for 40 on this workload.

Code tree at close: `05054d9c8f8add0ae950258bd343e56de1b5ef86` (stash
`665df5f12b30642683a88d6ea759988c0f41e108`; this docs stamp follows the code
tree). No commit.
