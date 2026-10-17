# Golden-master baselines

Reference screenshots of the original web-rendered UI, captured so the native
port can be compared against it. `manifest.json` sits beside the PNGs and
records what each shot shows, how it was captured, and which noise is expected.

## How this set was produced

- Command: `cd apps/desktop && bun run scripts/capture-baseline.ts --out <dir>`
- Tooling: `apps/desktop/scripts/capture-baseline.ts` (recipes) plus
  `apps/desktop/scripts/baseline-shots.json` (the 74 shot definitions). Both
  hashes and git blobs are recorded in `manifest.scripts`, so the exact capture
  revisions are recoverable.
- Source: `manifest.sourceCommit` names the tree the shots were taken from.
  `manifest.sourceDirty` records whether the worktree differed from it at
  capture time.
- Pins: `photrez.locale=en` (stable English strings) and
  `photrez.bgFlagRoute=0` — the application's own documented rollback switch.
  Without it the document-open path waits for a native seed that a browser
  cannot provide and refuses the open.
- Renderer: headless Chromium, ANGLE/SwiftShader software rasteriser,
  `deviceScaleFactor: 1`, viewport 1366x820. The renderer string is recorded in
  `manifest.host.renderer` — relevant to anyone comparing antialiased edges.
- Document recipe: a blank canvas named "Baseline", 800x600.

## Capture conventions (each one exists because a capture got it wrong once)

- **Locale pinned before boot** — selectors written from English text fail
  silently under another locale.
- **One fresh page load per shot** — leftover state (an open modal, a switched
  tool, a hover) makes a failure impossible to localise.
- **Tooltips suppressed by default** — an element screenshot is a region crop,
  not an isolation: anything portalled over the target gets baked into the PNG.
  Three tooltip shots opt back in (`allowTooltip` semantics in the script).
- **Rail disabled-state asserted** — the disabled rail applies
  `opacity-50 grayscale`, and `grayscale()` collapses colour to luma, so such a
  capture is irrecoverable in post. One shot deliberately captures the disabled
  state (`rail-disabled-no-doc`) to prove it cannot be reconstructed; every
  other shot asserts the rail is enabled.
- **Tauri invoke failures classified as expected noise** — in a browser every
  `invoke()` rejects; that is noise (`expectedNoise`, category
  `tauri-invoke-unavailable`), not breakage. A shot fails only on anything
  else.
- **A failing shot is recorded, the run continues** — per-shot `ok` plus a
  `failed` count in the summary.
- **Document open before capture** per `needsDoc` — the tool rail renders
  disabled without an active document.

## Tolerance policy (measured values)

| Rule | Value | Consequence for a diff |
|---|---|---|
| Neighbourhood tolerance | 3x3 on the semantic class map | masks any displacement of <= 1 px. `HARD MISMATCH = 0%` means "nothing moved by more than a pixel", not "nothing moved". |
| Antialiasing residual (accepted) | ink ratio 1.146x tools, 1.167x swatch; centre luminance identical, shoulders brighter | not a defect. It is how the rasterisers cover a sub-pixel stroke. |
| Colour changes are **not** tolerated | the 3x3 mask absorbs a pixel that moved, not one that changed colour | judge geometry via `HARD MISMATCH`, not raw `diff%`; regions with small type legitimately score a higher raw percentage. |
| Block anchoring | top-aligned comparison | anchor per-region, not globally; shells differ in height at the bottom. |
| Placeholder regions | dock/navigator numbers measured against a flat placeholder count "missing", not "wrong" | re-measure once the real dock exists. |

Producing the numbers above requires the reference tooling (`shape-diff` and
`measure` semantics: the class map with the 3x3 neighbourhood tolerance, the
ink integral, the bounding boxes). The port owns the Rust equivalent; two
tolerance implementations must never both be alive.

## Known run-to-run variance (measured)

Determinism was measured by re-capturing the set and diffing whole PNGs:
72 of 74 are byte-identical across runs. The two exceptions differ only inside
the status bar's live metrics segment (the numeric readout at the right of the
footer). Worst observed deltas, in each PNG's own coordinates:

| Shot | Differing region | Worst delta |
|---|---|---|
| `ctx-brush` (viewport 1366x820; footer occupies y 796-820) | x 720-869, y 801-816 | 665 px (0.06%) |
| `loading-status` (footer strip 1366x24) | x 620-994, y 5-20 | 767 px (2.3%) |

One recapture pair differed by only 127 px inside the same region, and the
`ctx-brush` recapture pair was identical — the variance lives there and
nowhere else. A comparison harness should mask that segment (or accept deltas
of this size there) and stay strict everywhere else.

### Second measurement (2026-10-17, clean-clone re-capture)

68 of 74 PNGs byte-identical; selector boxes drifted on 0 of 74 shots.
Three differing shots lie inside the metrics segment described above:
`shell-status-bar` 637 px, `dock-closed` 658 px (both x 764-912, y 801-815 in
viewport coordinates), `loading-status` 133 px inside the documented region.
The other three are sub-pixel antialiasing slivers outside it, absorbed by
the 3x3 neighbourhood rule: `dialog-resize` 4 px (worst channel delta 3),
`empty-full` 5 px (delta 3), `rail-disabled-no-doc` 8 px (delta 30).

## Acceptance rule

**A re-baseline is never accepted without a recorded reason.** Each manifest
shot entry carries a `reason` field; naming what changed and why it is intended
is the minimum. No reason, no commit — a silently re-baselined golden master is
a deleted test.

## Verifying a claim against this set

1. `manifest.json` is the index: find the shot, read its `note` and `box`.
2. Compare geometry first (`HARD MISMATCH` under the 3x3 class-map rule),
   colours second, raw diff% last.
3. Treat `expectedNoise` entries as expected; anything else is a finding.

## Size budget

Committed PNGs <= 5 MB, `manifest.json` <= 100 KB, directory total <= 6 MB.
If exceeded: split by category or re-capture — never compress with lossy
tooling, which changes the pixels being compared.
