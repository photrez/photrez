# Rust Pixel Pipeline — Performance Documentation

## Overview

This document consolidates performance findings from the C4/C5 Rust canonical pixel pipeline implementation.

## Pixel Parity (R1 Shadow)

### Divergence Summary

| Case | maxDiff | meanDiff | diffPixelPct |
|------|---------|----------|--------------|
| softBrush | 52 | 3.28 | 14.42% |
| hardBrush | 5 | — | — |
| pressureVaried | 93 | 2.70 | 15.60% |
| opacityFlow | 160 | 12.87 | 14.44% |
| eraserOnPainted | 228 | 11.10 | 47.19% |
| tileBoundaryCrossings | 28 | 0.192 | 36.63% |
| clippingEdges | 0 | 0 | BYTE-EXACT |

**Conclusion:** Divergence is consistent with premultiplied-blend/unpremultiply rounding (Skia vs fixed-point integer). Byte-exact achieved for clipping edges.

## Performance Attribution (R1.5)

### Bottleneck Breakdown (Stress: 6912×3888, brush 3000, 10 dabs)

| Stage | Time | Share |
|-------|------|-------|
| Blend (raster, premultiplied src-over) | 607–762ms | ~50% |
| Extraction (alloc + unpremul + traversal) | 398–465ms | ~30% |
| → unpremultiply MATH alone | ≈310–365ms | ~23% |
| → raw PM copy floor | 87–101ms | ~7% |

### Rayon Experiment Results

| Stage | Serial | Parallel | Speedup |
|-------|--------|----------|---------|
| Extraction | 398–465ms | 96–116ms | 4.3–4.6× |
| Blend | 589–839ms | 219–294ms | 2.7–2.85× |

**Status:** Experiment only. No production Rayon committed.

### Projected Performance

If both stages parallelized: stress totalRust 10.6s → ~0.9–1.2s

Still ≥2–3× above TS baseline (365ms for v2 Batched).

## Final Validation Results

### Workload Performance (ms, p50)

| Workload | Current | Incremental | Batched |
|----------|---------|-------------|---------|
| 32×3000 dense | 657 | 3866 | **155** |
| 32×3000 sparse | 548 | 3216 | **51** |
| 512×300 dense | 323 | 1227 | **200** |
| 512×300 sparse | 231 | 958 | **73** |
| 1024×100 dense | 272 | 929 | **225** |
| 2048×30 dense | 293 | 757 | **196** |

**Conclusion:** Batched total is fastest across all workloads.

## Known Limitations

1. Sampling divergence = first 3 tiles with different hash per case (≤256KB each), not full surface.
2. Byte-shipping disabled for cases >8MB changed bytes → only hash-tier + sample.
3. Rayon parallelization is experiment-only, not committed to production.

## What Remains NEEDS MEASUREMENT

- Full-layer readback cost at 4K resolution
- Band-size sweep + scaling curve vs core count for Rayon
- Production Rayon integration impact
- Memory implications of parallel extraction

---

*Consolidated from: bench-final-validation, parity-r1-inshell-report, r15-attribution-and-divergence, bench-adaptive-*, bench-brush-*, bench-incremental-*, bench-snapshot-cow-*, bench-tip-cache-*, bench-large-brush-*, bench-real-tip-*, bench-brush-pixels-*, bench-brush-micro-*, bench-final-audit-*, bench-final-matrix-v2-*, r15-projection-reconciliation*
