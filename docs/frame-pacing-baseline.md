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

## Status: FAILS on the current tree — a code regression

The requirement **fails** on HEAD, in every mode measured. Steady-state
percentiles are perfect (median 16.7 ms, p95 16.8–16.9 ms — exactly vsync), but
a single multi-second stall lands during brush interaction, three orders of
magnitude over budget.

**The cause is attributed: it is the code delta, not the build route.** See
"Attribution" below — this was established by rebuilding the older tree through
the identical toolchain, route and harness.

### HEAD `34dad0cd` — **FAILS**

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
`b62c60d8..34dad0cd`** — roughly 50-56 commits. The build route is exonerated:
the older tree built through the same route on the same machine in the same
session is clean, 3-for-3.

## Bisect: `9dc10677` introduces the stall

**`9dc10677` — `fix(editor): fence the brush on Rust store presence, not graph ownership` (2026-10-02)** is the first bad commit. It is the **24th of 56** commits in `b62c60d8..34dad0cd`; its parent is `99afe618`.

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

### What the commit changed (mechanism — HYPOTHESIS, not measured)

`9dc10677` changes no pixel code. It replaces the brush's entry gate in
`apps/desktop/src/components/editor/useBrushOverlay.ts`:

- **Before** (`useBrushOverlay.ts` at `9dc10677^`), Gate A refused the stroke outright on any facade-owned layer: `if (isFacadeOwnedLayer(activeId)) { showToast(...); return; }`. On a newly created document the layer is facade-owned, so **the stroke was refused and the entire downstream commit never ran** — which is why the "good" commits measure 17 ms.
- **After** (`useBrushOverlay.ts:717-730` and `:1131-1143`), that refusal is replaced by a per-stroke store-presence fence, `strokeBlockedByRust` (`useBrushOverlay.ts:174-187`), which blocks only on a positive "Rust does not hold this layer" and otherwise lets the stroke through.

So the commit does not *add* work; it **un-gates a path that was previously unreachable in this scenario**, and that path is expensive on a large document:

1. First stroke finds no Rust storage, so it seeds the store with a **full-layer readback**: `surface.readRect(0, 0, w, h)` then `rust_pixels_init` (`useBrushOverlay.ts:217-220`) — 67 MB of pixels for 4096².
2. The commit syncs the post-stroke surface back into the model via `surface.toImageBitmap()` + `engine.setLayerImageBitmap` (`useBrushOverlay.ts:276-285`).
3. `setLayerImageBitmap` unconditionally calls `pushModelToRust` (`apps/desktop/src/engine/document.ts:1455`), which `JSON.stringify`s the whole model and calls `restore_snapshot` (`document.ts:1767-1792`).

**This is a mechanism hypothesis with `file:line`, distinct from the measured attribution.** The attribution — that `9dc10677` is the first commit where the stall appears — is measured 3-for-3 on both sides. The claim that the full-layer seed plus the model replay is what costs the 16 seconds is read off the source, not profiled. Confirming it needs a per-phase timing run inside the first stroke; **no such run was made, so the 16 s is not apportioned between the three steps above.**

## Where the stall is (localised)

A phase-tagged continuous rAF sampler ran across the whole session — from before
the document dialog opened to after the last stroke — attributing every frame to
the phase active when it began. Times are ms from sampler start.

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

The introducing commit is now named (`9dc10677`, see "Bisect" above), measured at
the boundary with n=3 on each side. The `file:line` mechanism that explains *why*
that commit is expensive is still labelled a hypothesis and is **not** claimed as
proven: the 16 s has not been apportioned between the full-layer Rust seed, the
model-bitmap sync, and the whole-model replay.

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
  each side. Its *mechanism* is a hypothesis with `file:line`, not a measurement:
  no per-phase profile of the first stroke was taken, so the 16 s is not
  apportioned between the full-layer seed, the bitmap sync, and the model replay.
- **Not fixed.** This record locates the regression; the repair is a separate,
  reviewed change.
- **The bisect measured one configuration.** Cold 4096², brush, default flags,
  release profile, `photrez.tileCommit` left at its default ON. It says nothing
  about other document sizes, the eraser, or a flag set that skips the Rust path.

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