# ARCHITECTURE.md — Photrez (Runtime Reference)

---

## Overview

Photrez is a lightweight desktop image editor built for practical digital and print workflows.

**MVP Runtime:** Tauri 2 (shell) + SolidJS/TypeScript (frontend) + **TypeScript DocumentEngine** (core) + **WebGL2** (renderer).
**Future target:** Rust Core via WASM (photrez-core → wasm-pack) for hot-path compute (brush, transform, tile, encode) + WebGL2 remains the renderer. wgpu deferred until compute-shader features are required.
**Revised 2026-08-18:** Interactive pixel compute now targets **browser WebGPU (WGSL compute shaders)** for realtime/no-delay; **native Rust** retained for export encode (entropy coding is serial → CPU). WASM hot-path compute deferred. See GPU Compute Layer below.

---

## Project Status

- **Phase**: Deep-sync & documentation hardening (2026-07-02). Bun migration, mojibake cleanup, print feature, keyboard shortcut expansion, window-state persistence, E2E grand-tour, and multi-cycle bug-fix passes completed.
- **Core Crate**: `photrez-core` is the WASM export encoder only (PNG/JPEG/WebP via the `image` crate), bound as `encode_image_wasm`. The domain model (document/layer/selection/transform/brush/workspace/history) was pruned in Phase 1 — app state lives in the TypeScript `DocumentEngine`. Core crate has 0 Rust tests (encoder exercised via TS `wasmExport`/`exportDocument`); desktop crate retains 25 tests.
- **Renderer**: MVP rendering is **WebGL2** (`apps/desktop/src/renderer/webgl2.ts`). A standalone GPU crate was evaluated but removed from the workspace (a pre-existing Windows entry-point issue); the future rendering direction keeps WebGL2 and only reintroduces a GPU crate when compute shaders or advanced blend modes exceed WebGL2 capabilities.
- **WASM Strategy (2026-06-30)**: `photrez-core` is the source for WASM compilation (`wasm-pack` target). Hot-path operations (brush mask, tile ops, transform math, color space, export encode) will be ported from TypeScript to Rust and exposed as zero-copy WASM modules — no Tauri IPC overhead.
- **Frontend**: Full UI shell with multi-document workspace, document tabs, empty state, drag/drop, and all core editing interactions. Artboard renders via WebGL2 projection-matrix-driven camera viewport.
- **Testing**: Verified gates are enforced via CI (frontend type-check + unit/component tests, Rust `cargo test`, and Playwright E2E). See `README.md` and `.github/workflows/ci.yml` for the current pipeline.
- **Recovery Reference** (historical): see the project archive maintained outside this repository.

---

## Technology Stack

| Layer            | Technology                                              |
| ---------------- | ------------------------------------------------------ |
| Desktop Shell    | Tauri 2                                                |
| Frontend         | SolidJS + TypeScript (TSX)                             |
| Build Tool       | Vite 8                                                 |
| Styling          | Tailwind CSS v4 (`@theme` based tokens)                |
| Core Engine (MVP) | TypeScript `DocumentEngine` (`apps/desktop/src/engine/document.ts`) |
| Core Engine (future) | Rust `photrez-core` compiled via WASM (wasm-pack) — zero-copy from TS, no IPC |
| GPU Renderer (MVP) | WebGL2 (`apps/desktop/src/renderer/webgl2.ts`)          |
| GPU Renderer (future) | WebGL2 remains; wgpu deferred until compute-shader features required |
| Compute (future hot-path) | **Browser WebGPU/WGSL** compute shaders for interactive pixel ops (invert/adjust/filter); **native Rust** for export encode; CPU fallback when WebGPU absent |
| State (Backend)  | `tauri::State<'_, T>` + `Mutex` (Rust managed state)  |
| State (Frontend) | SolidJS `createSignal` / `createStore`                 |
| Package Manager  | Bun (monorepo workspace)                               |
| Icons            | `lucide-solid` package                                 |

---

## Architecture Diagram

### Active MVP Runtime (2026-07-02)

This is the current runtime ownership map. Use this diagram and the registered command table as the source of truth for review.

```text
User input / UI events
        |
        v
SolidJS editor shell
  - App chrome, tabs, panels, tools
  - Solid signals and stores for UI state
  - WorkspaceManager + TypeScript DocumentEngine for MVP document state
        |
        +--> WebGL2 renderer
        |      - texture upload
        |      - compositing / preview
        |      - viewport readback where required
        |
        +--> Browser WebGPU compute (future hot-path, WGSL)
        |      - interactive pixel ops: invert, adjust, filter, flatten
        |      - CPU fallback when navigator.gpu absent
        |      - shaders in apps/desktop/src/lib/gpu/shaders/*.wgsl
        |
        +--> Native Rust (export encode, serial codec)
        |              - PNG/JPEG/WebP/TIFF via `image` crate
        |      - zero-copy bytes from TS / Tauri IPC
        |
        +--> Tauri 2 shell commands (cold-path only)
               - ping
               - get_contract_info
               - read_file_bytes  (image import allowlist, 256MB cap)
               - write_file_bytes (image export allowlist, 256MB cap)

Rust crate:
  - photrez-core: export encoder only (WASM), no domain model/tests after Phase 1 prune
```

### Historical / Future-Target Reference

Note 2026-06-19: the diagram below is historical and retained only for ownership-context archaeology. It still contains earlier workspace/layer/history command labels that are not registered in the current Tauri runtime. Do not use it as the active runtime command map.

```text
┌──────────────────────────────────────────────────────────────┐
│                    FRONTEND PROCESS                           │
│                    (SolidJS + Vite)                           │
│                                                               │
│  ┌──────────────────┐      ┌───────────────────────────────┐ │
│  │   App Shell       │      │     Canvas Viewport           │ │
│  │  (UI Chrome)      │      │  (IPC base64 preview)         │ │
│  │                   │      │                               │ │
│  │ - Menubar (44px)  │      │ - Ruler bars                  │ │
│  │ - Toolbar (40px)  │      │ - Artboard preview            │ │
│  │ - Tab Strip (30px)│      │ - Transform handles           │ │
│  │ - Tool Rail       │      │ - Selection overlay           │ │
│  │ - Inspector       │      │ - Empty state (no docs)       │ │
│  │ - Status Bar      │      │                               │ │
│  └────────┬──────────┘      └──────────┬────────────────────┘ │
│           │                            │                      │
│  ┌────────┴────────────────────────────┴──────────────────┐  │
│  │              SolidJS Reactive State                     │  │
│  │  createSignal: activeTool, zoom, mousePos, layers       │  │
│  │  createSignal: documents, activeDocumentId, limits      │  │
│  └───────────────────────────┬────────────────────────────┘  │
│                              │ invoke() from                  │
│                              │ @tauri-apps/api/core           │
└──────────────────────────────┼────────────────────────────────┘
                               │ Tauri IPC Bridge
                               │ (automatic serialization/deserialization)
┌──────────────────────────────┼────────────────────────────────┐
│                    TAURI SHELL (Rust)                          │
│                              │                                │
│  ┌───────────────────────────┴────────────────────────────┐  │
│  │              main.rs — Command Handlers                 │  │
│  │           #[tauri::command] functions                    │  │
│  └──────┬─────────────┬──────────────┬────────────────────┘  │
│         │             │              │                        │
│  ┌──────┴─────┐  ┌────┴─────┐  ┌─────┴──────┐                │
│  │ Workspace  │  │ Layer    │  │ History    │                │
│  │ Commands   │  │ Commands │  │ Commands   │                │
│  │ get_ws_st  │  │ add_layer│  │ undo       │                │
│  │ open_images│  │ delete   │  │ redo       │                │
│  │ switch_doc │  │ reorder  │  │            │                │
│  │ close_doc  │  │ update   │  │            │                │
│  └──────┬─────┘  └────┬─────┘  └─────┬──────┘                │
│         │             │              │                        │
│  ┌──────┴─────────────┴──────────────┴────────────────────┐  │
│  │              AppRuntime (tauri::manage)                  │  │
│  │  ┌─────────────────────┐  ┌─────────────────────────┐  │  │
│  │  │ Mutex<WorkspaceState>│  │ Mutex<ViewportState>     │  │  │
│  │  │ (photrez-core)       │  │ (presentation state)     │  │  │
│  │  │ - documents[]        │  │ - artboard pos/size      │  │  │
│  │  │ - active_document_id │  │ - pan/zoom               │  │  │
│  │  │ - per-doc state      │  │                          │  │  │
│  │  └─────────────────────┘  └─────────────────────────┘  │  │
│  └─────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────┘

┌───────────────────────────────────────────────────────────────┐
│                    RUST CRATES                                 │
│                                                               │
│  ┌─────────────────────────────┐                             │
│  │ photrez-core                 │                             │
│  │ (crates/core/)               │                             │
│  │                              │                             │
│  │ ├── document.rs              │                             │
│  │ │   Document, layers,        │                             │
│  │ │   selection, transform     │                             │
│  │ ├── layers.rs                │                             │
│  │ │   Layer, BitmapData        │                             │
│  │ ├── history.rs               │                             │
│  │ │   HistoryStore, undo/redo  │                             │
│  │ ├── workspace.rs ★ NEW       │                             │
│  │ │   WorkspaceState           │                             │
│  │ │   DocumentSession          │                             │
│  │ ├── brush.rs                 │                             │
│  │ ├── export.rs                │                             │
│  │ └── lib.rs                   │                             │
│  └──────────────────────────────┘                             │
└───────────────────────────────────────────────────────────────┘
```

---

## Data Flow

### Hot Path (WASM / TypeScript — no IPC)

```text
1. User action (click/drag/shortcut) in SolidJS frontend
        ↓
2. TS DocumentEngine mutates state synchronously (zero IPC)
        ↓
3. Future: WASM module called for compute-heavy ops:
   - brushMask(radius, hardness, size)   → Uint8Array
   - splitIntoTiles(bitmap, tileSize)    → Tile[]
   - composeFromTiles(tiles, w, h)        → ImageBitmap
   - exportEncode(pixels, format, quality) → Uint8Array
        ↓
4. history.commit(snapshot) before mutation
        ↓
5. scheduler.requestRender() → WebGL2 next frame
```

### Cold Path (Tauri IPC — file I/O only)

```text
1. User triggers file open / save / export
        ↓
2. Frontend memanggil invoke("read_file_bytes" / "write_file_bytes")
        ↓
3. Tauri IPC bridge forwards to #[tauri::command] handler
        ↓
4. Rust reads/writes file from disk via std::fs
        ↓
5. Returns base64-encoded bytes (256MB cap)
        ↓
6. Frontend decodes, creates ImageBitmap (import) or Blob (export)
```

---

## Response Envelope Contract (v2.0.0)

```rust
// Success
{ "ok": true,  "contract_version": "2.0.0", "data": { ... } }

// Error
{ "ok": false, "contract_version": "2.0.0", "error": { "code": "...", "message": "...", "details": null } }
```

Detail lengkap: `docs/reference/command-contract-spec.md`. The current Tauri shell runtime exposes a small file-IO contract; most editor operations remain in the TypeScript MVP hot path (and future WASM compute hot path) and are not registered as Tauri commands.

---

## Registered Tauri Commands (Active Runtime)

| Command | Params | Module | Status |
| --- | --- | --- | --- |
| `ping` | none | Shell | Active |
| `get_contract_info` | none | Shell | Active |
| `read_file_bytes` | `path: String` | Shell file IO | Active, image import extension allowlist, 256MB cap |
| `write_file_bytes` | `path: String, data: String` | Shell file IO | Active, image export extension allowlist, base64 payload, 256MB cap |

Historical workspace, layer, crop, and export command names describe earlier Rust-command plans and current product capabilities, but they are not registered in `apps/desktop/src-tauri/src/main.rs` in the MVP runtime. Their active implementation path is the SolidJS/TypeScript editor engine plus WebGL2 renderer, with Tauri used for shell file IO and dialogs.

---

## File Structure (Key Paths)

```
photrez/
├── apps/
│   └── desktop/
│       ├── src/                    # SolidJS Frontend
│       │   ├── App.tsx             # Main app component (51 lines, delegates to EditorShell)
│       │   ├── index.css           # Tailwind v4 + design tokens
│       │   ├── styles.css          # Additional component styles
│       │   ├── index.tsx           # Entry point
│       │   ├── engine/             # TypeScript DocumentEngine
│       │   │   ├── document.ts     # Core document model + operations
│       │   │   ├── history.ts      # Command history (undo/redo)
│       │   │   ├── workspace.ts    # Multi-document workspace
│       │   │   └── ...             # cropApply, layerComposite, etc.
│       │   ├── renderer/           # WebGL2 rendering pipeline
│       │   │   ├── webgl2.ts       # WebGL2Backend renderer
│       │   │   ├── shaders.ts      # GLSL ES 3.0 shaders
│       │   │   └── scheduler.ts    # Render scheduling + continuous mode
│       │   ├── viewport/           # Viewport camera + geometry modules
│       │   │   ├── viewportCamera.ts      # ViewportCamera class + easing
│       │   │   ├── modernCropGeometry.ts  # Modern crop frame geometry
│       │   │   ├── cropGeometry.ts        # Classic crop geometry
│       │   │   ├── cropSnap.ts            # Crop snap targets
│       │   │   ├── smartGuides.ts         # Smart guide line system
│       │   │   ├── transformGeometry.ts   # Transform handle geometry
│       │   │   └── ...                    # coords, easing, hitTest, etc.
│       │   └── components/editor/  # 94 editor components + 76 test files
│       └── src-tauri/
│           ├── src/
│           │   ├── main.rs         # App entry + builder (59 lines)
│           │   ├── commands.rs     # Tauri command handlers (read/write/print)
│           │   ├── response.rs     # Response envelope types + validation
│           │   ├── window_state.rs # Window state persistence
│           │   └── menu.rs         # Native menu construction
│           ├── Cargo.toml          # Tauri dependencies
│           └── tauri.conf.json     # Tauri configuration
├── crates/
│   ├── core/                       # photrez-core (WASM export encoder only, after Phase 1 prune)
│   │   └── src/
│   │       ├── lib.rs              # Crate root
│   │       ├── document.rs         # Document struct & operations
│   │       ├── layers.rs           # Layer struct & serialize
│   │       ├── history.rs          # HistoryStore (undo/redo)
│   │       ├── workspace.rs        # WorkspaceState + DocumentSession
│   │       ├── selection.rs        # Selection operations
│   │       ├── transform.rs        # Transform operations
│   │       ├── brush.rs            # Brush stroke operations
│   │       └── export.rs           # Export encode (JPG/PNG/WebP)
├── docs/                           # Project documentation (see INDEX.md)
│   ├── INDEX.md                    # ★ Documentation routing guide
│   ├── FEATURES.md                 # ★ Feature status tracker
│   ├── ARCHITECTURE.md             # ★ This file
│   ├── CONVENTIONS.md              # ★ Code patterns & domain knowledge
│   ├── UI_GUIDE.md                 # ★ Consolidated UI design reference
│   ├── spec/product-scope.md       # MVP scope lock
│   ├── spec/prd.md                 # Product requirements
│   ├── reference/design-tokens.md  # Design token values
│   ├── reference/command-contract-spec.md # IPC contract spec
│   ├── ...                         # Reference docs (see INDEX.md)
│   └── archive/                    # Archived planning & history docs
├── AGENTS.md                       # Agent orchestration rules
├── GEMINI.md                       # Visual aesthetic rules
├── CLAUDE.md                       # Claude-specific rules
└── Cargo.toml                      # Rust workspace root
```

---

## Architecture Rules (MANDATORY)

### Module Boundaries

| Module            | Owns                                                    | Must NOT Own                            |
| ----------------- | ------------------------------------------------------- | --------------------------------------- |
| Shell (Tauri)     | App lifecycle, file dialogs, cold-path file I/O          | Image processing, document state logic  |
| Core (Rust WASM)  | Hot-path compute: brush mask, tile ops, transform math, color conversion, export encode | Rendering, UI state, file I/O |
| Renderer (WebGL2) | Frame rendering, texture upload, compositing             | Persistence, document rules, UI state   |
| Frontend (Solid)  | UI state (tool, zoom, panel), user interaction, TS DocumentEngine | Document truth (MVP), pixel manipulation (future: WASM) |

### Source of Truth

- **Document state (MVP)**: In TypeScript `DocumentEngine` (`apps/desktop/src/engine/document.ts`). After Phase 1 prune, `photrez-core` is the WASM export encoder only (no domain model / no tests) — app state lives in TS.
- **Document state (future)**: Stays in TS `DocumentEngine`. Rust WASM modules are called for compute, not ownership — no dual-state synchronization problem.
- **UI state**: In SolidJS signals (tool selection, zoom level, panel visibility).
- **Pixel data (MVP)**: `ImageBitmap` per layer in `DocumentEngine`, rendered by WebGL2. Future WASM modules operate on `Uint8Array` zero-copy from JS.
- **NEVER** duplicate document state in the frontend as a mutable source.

---

## Performance Targets

| Metric         | Target       |
| -------------- | ------------ |
| Installer size | `< 80 MB`   |
| Idle RAM       | `< 250 MB`  |
| Startup time   | `< 2s`      |

Measurement protocol: `docs/reference/performance-measurement-protocol.md`

---

## GPU Compute Layer (2026-08-18)

Interactive per-pixel compute (invert, adjustments, filters) runs on the **GPU via WebGPU compute shaders written in WGSL**. Layer flatten/transform are compositing (Canvas2D export + WebGL2 live preview) and are already GPU-backed — they are not WGSL compute ops. Brush stroke commit (`useBrushOverlay.commitBrushStroke`) composites dabs via Canvas2D `drawImage` (platform-GPU) + `createImageBitmap` — also already GPU-backed, not a WGSL candidate. **Scope delivered:** invert (G1) + adjustments B/C/S (G2) as WGSL; flatten/transform (G3) and brush (G4) evaluated N/A; **GPU/Rust optimization track closed** (no further WGSL/Rust win without benchmark proof). Evidence-backed rationale:

- Browser WebGPU accepts **WGSL only** (Chrome 117 dropped SPIR-V ingestion); `rust-gpu` (SPIR-V) does not run directly in the Tauri WebView2. Hand-written WGSL is the correct "build-it-ourselves" form.
- Benchmark on real AMD Radeon (Deno native WebGPU / wgpu→D3D12): invert 4K (12M px) = **GPU 3.73ms vs TS 21.49ms ≈ 5.9×, correct=true**; 3.73ms << 16.6ms frame budget → realtime/no-delay proven.
- Image pixel ops are embarrassingly parallel → GPU territory. A naive `&[u8]→Vec<u8>` WASM port regressed to **0.56×** (copy-bound); zero-copy wasm reached only **0.98×** parity. GPU wins for interactive.

**Split:**
- Interactive compute → WGSL GPU compute (realtime). CPU fallback retained when `navigator.gpu` is absent.
- Export → native Rust (`photrez-core` + `image` crate). The pixel-flatten/transform step may run on GPU; the **codec encode** (PNG/JPEG/WebP entropy coding) is serial and stays on CPU/native Rust for correctness + maturity.

**Module:** `apps/desktop/src/lib/gpu/` — `GpuCompute` wrapper (WebGPU pipeline + CPU fallback), shaders in `shaders/*.wgsl` imported via Vite `?raw`.

**Constraint:** WGSL shaders are hand-written (not `wgsl-rs`) for now — minimal build surface. Revisit `wgsl-rs` (Rust→WGSL) only if source-language consistency becomes a hard requirement.
