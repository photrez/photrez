// apps/desktop/src/test/setup.ts
//
// Global Vitest setup file. Minimal: adds jest-dom matchers
// and per-test DOM isolation. Per-test mocks added in individual
// test files as needed.
//
// (Global setup follows the project test-overhaul conventions.)
//
// ── Experimental findings (2026-06-14) ─────────────────────────
// The following global mocks were tried and REVERTED because they
// broke existing tests:
//
//   • Element.prototype.setPointerCapture mock via Object.defineProperty
//     — jsdom 29 has it as a read-only property. defineProperty throws
//       "Cannot assign to read only property" in strict mode. Production
//       code (useSelectionTransformDrag.ts) already wraps calls in
//       try/catch per P0-7/P0-8 fix, so missing/broken pointer capture
//       in jsdom does not crash tests.
//
//   • HTMLCanvasElement.prototype.getContext mock returning {} for webgl2
//     — Broke LayersPanel/exportDocument/SelectionManager tests that
//     depend on real bitmap data flow. Use per-test mock instead.
//
//   • globalThis.getComputedStyle mock returning "" for all properties
//     — Broke tests that assert on real element positioning/transforms.
//
//   • globalThis.requestAnimationFrame mock that stores callbacks without
//     running them — Broke CanvasViewport (29 tests) and scheduler tests
//     that depend on default RAF async behavior. Use jsdom default.
//
//   • vi.clearAllMocks() in beforeEach — Cleared spies created in test
//     file's own beforeEach (e.g. setViewportSpy in Navigator.test.tsx).
//     Test files that need mock reset use vi.clearAllMocks() explicitly.
//
//   • sequence.concurrent: true — Ran multiple test files in parallel
//     within a single worker thread, causing state pollution with
//     vite-plugin-solid. 67 tests regressed. Keep sequence default.
//
// ── lucide-solid mock ──────────────────────────────────────────
// WARNING: vi.mock("lucide-solid") does NOT work with bun — bun's native
// module loader bypasses Vitest's mock system by resolving modules directly
// from the .bun cache (~/node_modules/.bun/lucide-solid@...). Neither
// vi.mock, Vite resolve.alias, nor Vite resolveId plugins can intercept this.
// This affects ALL component-jsdom tests, not just specific test files.
// Fix pending: either bun fixes module resolution integration with Vitest,
// or we restructure test dependencies to avoid lucide-solid in the module
// graph of test files (e.g., extract icons into a test-safe wrapper).

import { beforeEach, afterEach, vi } from 'vitest';
import '@testing-library/jest-dom/vitest';

beforeEach(() => {
  document.body.innerHTML = '';
  // jsdom 29 does not implement elementFromPoint — useCanvasLayerDrag relies
  // on it for tab hover detection during canvas drag. Default to null so
  // per-test mocks can override.
  if (typeof document.elementFromPoint !== "function") {
    (document as any).elementFromPoint = vi.fn().mockReturnValue(null);
  }
});

afterEach(() => {
  document.body.innerHTML = '';
});
