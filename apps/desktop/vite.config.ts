/// <reference types="vitest" />
import { defineConfig } from "vitest/config";
import solidPlugin from "vite-plugin-solid";
import tailwindcss from "@tailwindcss/vite";
import wasm from "vite-plugin-wasm";

// Keep this list conservative: only tests that do not require browser globals,
// Solid rendering, canvas APIs, or DOM event wiring belong in the fast project.
const nodeTestFiles = [
  "src/engine/__tests__/paintHistoryBudget.test.ts",
  "src/engine/__tests__/blendModes.test.ts",
  "src/engine/__tests__/history.test.ts",
  "src/engine/__tests__/errorResilience.test.ts",
  "src/engine/__tests__/document.test.ts",
  "src/engine/__tests__/cropUndoIntegration.test.ts",
  "src/engine/__tests__/postCropAlignment.test.ts",
  "src/engine/__tests__/undoEdgeCases.test.ts",
  "src/engine/__tests__/workspace.test.ts",
  "src/engine/__tests__/tileStorage.test.ts",
  "src/components/editor/__tests__/dragTypes.test.ts",
  "src/components/editor/__tests__/toolLifecycle.test.ts",
  "src/components/editor/__tests__/brushReferenceAudit.test.ts",
  "src/components/editor/__tests__/rotateBand.test.ts",
  "src/components/editor/__tests__/brushToolState.test.ts",
  "src/components/editor/__tests__/paintSmoothing.test.ts",
  "src/components/editor/__tests__/pointerCapture.test.ts",
  "src/components/editor/__tests__/crossDocLayerOps.test.ts",
  "src/components/editor/__tests__/transformSession.test.ts",
  "src/components/editor/__tests__/pasteboardClickPolicy.test.ts",
  "src/components/editor/__tests__/paintStrokeRenderer.test.ts",
  "src/components/editor/__tests__/paintStrokeCoordinates.test.ts",
  "src/components/editor/__tests__/paintCommitCommand.test.ts",
  "src/components/editor/__tests__/exportDocument.test.ts",
  "src/components/editor/__tests__/crossDocLayerOps.engine.test.ts",
  "src/components/editor/__tests__/brushTipMask.test.ts",
  "src/features/selection/__tests__/SelectionValidator.test.ts",
  "src/features/selection/__tests__/SelectionOperations.test.ts",
  "src/features/fill/__tests__/fillOperations.test.ts",
  "src/renderer/__tests__/webgl2-scissor.test.ts",
  "src/ui-sanity.test.ts",
  "src/__tests__/viewport.test.ts",
  "src/__tests__/layer-hit-test.test.ts",
  "src/__tests__/snap-adjustment.test.ts",
  "src/__tests__/transform.test.ts",
  "src/__tests__/smart-guides.test.ts",
  "src/__tests__/move-rotate-cursor.test.ts",
  "src/__tests__/renderer.test.ts",
  "src/__tests__/modern-crop-state.test.ts",
  "src/__tests__/modern-crop-geometry.test.ts",
  "src/__tests__/input-handler-move.test.ts",
  "src/__tests__/input-handler-snap.test.ts",
  "src/__tests__/cursor-rotate.test.ts",
  "src/__tests__/input-handler-selection.test.ts",
  "src/__tests__/crop-snap.test.ts",
  "src/__tests__/crop-geometry.test.ts",
  "src/__tests__/transform-geometry.test.ts",
  "src/__tests__/history-audit.test.ts",
  "src/__tests__/keyboard-shortcuts.test.ts",
  "src/__tests__/cursor-resolver.test.ts",
] as const;

const defaultTestExcludes = ["node_modules/**", "dist/**", "e2e/**"];

export default defineConfig({
  plugins: [
    solidPlugin(process.env.VITEST ? { hot: false } : undefined),
    tailwindcss(),
    wasm(),
  ],

  test: {
    projects: [
      {
        // extends:true inherits the root plugin chain (solidPlugin + tailwind)
        // and resolve/tsconfigPaths. Without it the projects lose solidPlugin,
        // so Solid JSX is never transformed and component tests fail to parse.
        extends: true,
        test: {
          name: "unit-node",
          globals: true,
          environment: "node",
          include: [...nodeTestFiles],
          exclude: defaultTestExcludes,
          css: true,
          pool: "threads",
          isolate: true,
        },
        esbuild: {
          jsx: "automatic",
          jsxImportSource: "solid-js",
        },
        resolve: {
          tsconfigPaths: true,
        },
      },
      {
        test: {
          name: "component-jsdom",
          globals: true,
          environment: "jsdom",
          include: ["src/**/*.test.{ts,tsx}"],
          exclude: [...defaultTestExcludes, ...nodeTestFiles],
          setupFiles: ["./src/test/setup.ts"],
          css: false,
          pool: "forks",
        },
        extends: true,
        esbuild: {
          jsx: "automatic",
          jsxImportSource: "solid-js",
        },
        resolve: {
          tsconfigPaths: true,
        },
      },
    ],
  },

  // Root-level Vite config (shared by projects where not overridden)
  esbuild: {
    jsx: "automatic",
    jsxImportSource: "solid-js",
  },

  resolve: {
    tsconfigPaths: true,
  },

  // Prevent vite from obscuring rust errors
  clearScreen: false,

  // Tauri expects a fixed port, fail if that port is already in use
  server: {
    strictPort: true,
    port: 1420,
    host: "0.0.0.0"
  },

  // to make use of `TAURI_PLATFORM`, `TAURI_ARCH`, `TAURI_FAMILY`,
  // `TAURI_PLATFORM_VERSION`, `TAURI_PLATFORM_TYPE` and `TAURI_DEBUG`
  // env variables in vite
  envPrefix: ["VITE_", "TAURI_"],

  build: {
    // Tauri supports es2021, but esnext ensures latest features work without transform errors
    target: "esnext",
    // don't minify for debug builds
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    // produce sourcemaps for debug builds
    sourcemap: !!process.env.TAURI_DEBUG,
    // Split stable, heavy groups out of the main index chunk so no single
    // file exceeds the 500 kB performance budget (scripts/performance-budget.mjs).
    rolldownOptions: {
      output: {
        manualChunks(id: string) {
          if (id.includes("node_modules/solid-js") || id.includes("node_modules/solid-primitives")) return "vendor-solid";
          if (id.includes("/src/engine/")) return "engine";
          if (id.includes("/src/renderer/")) return "renderer";
        },
      },
    },
  },
});
