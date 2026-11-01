// SPDX-License-Identifier: AGPL-3.0-or-later
//! Canonical pixel owner, namespaced by document id. This is the long-term owner
//! (replacing the flat process-global `PIXEL_STORE`). Lifecycle is driven from
//! TS via the `rust_pixels_*` commands: open/close/add/remove/resize manage
//! entries; a missing document namespace is created lazily on first layer init.
//!
//! The registry owns one `DocumentPixelStore` per document. That store owns the
//! canonical `PixelLayer` buffers, the per-layer `StateNode` canon, and the
//! document's authoritative history stream.
//!
//! Invariants carried over from the former flat `pixel_store` module:
//! - exactly one `PixelLayer` buffer exists per layer;
//! - every canonical mutation bumps the layer epoch by one;
//! - the per-layer `StateNode` canon is re-seeded whenever the row-major buffer
//!   is written outside the COW path (desync guard).

use std::collections::HashMap;
#[cfg(any(feature = "legacy-host", test))]
use std::sync::{Mutex, MutexGuard};

use super::document_store::DocumentPixelStore;

/// Migration flag. OFF (default): the row-major `PixelLayer` is the ACTIVE
/// production canonical and the TS-facing behavior is unchanged. The
/// `LayerState`/`StateNode` path is AVAILABLE (history entries always store
/// `Arc<StateNode>`) but NOT activated as the byte source.
/// ON: the per-layer `LayerState` becomes the canonical reader for the pixel
/// path (forward path still mirrors into `PixelLayer` so the existing
/// `get_layer` accessor stays stable).
#[allow(dead_code)]
const STATE_NODE_CANONICAL: bool = false;

/// Migration flag — the packed Arc-subarray canonical store.
/// OFF (default): row-major `PixelLayer` is the ACTIVE production byte source;
/// the packed-backed `TileStore` is the pack/upload store only (the state_node
/// `LayerState` packed COW seam is built lazily for history but not the byte
/// source). ON: the packed tile-major `TileStore` becomes the canonical byte
/// source. A single runtime const (analogous to `STATE_NODE_CANONICAL`); the
/// row-major `PixelLayer` remains the rollback path. Flipping to `true` requires
/// the TS mirror wiring + tests first.
#[allow(dead_code)]
const TILE_MAJOR: bool = false;

/// Canonical pixel owner, namespaced by document id. This is the long-term owner
/// (replacing the flat process-global `PIXEL_STORE`). Lifecycle is driven from
/// TS via the `rust_pixels_*` commands: open/close/add/remove/resize manage
/// entries; a missing document namespace is created lazily on first layer init.
#[cfg_attr(
    not(feature = "legacy-host"),
    doc = r#"

The default core build exposes typed store operations but not the legacy global
registry or its raw mutable internals.

```compile_fail
use photrez_core::pixel::registry;
fn main() { let guard = registry(); drop(guard); }
```

```compile_fail
use photrez_core::pixel::store::registry;
fn main() { let guard = registry(); drop(guard); }
```

```compile_fail,E0603
use photrez_core::pixel::DocumentPixelStore;
fn main() { let _ = DocumentPixelStore::default(); }
```

```compile_fail,E0616
use photrez_core::pixel::PixelStoreRegistry;
fn main() {
    let mut store = PixelStoreRegistry::new();
    store.docs.clear();
}
```

```compile_fail,E0603
use photrez_core::pixel::document_store::DocumentPixelStore;
fn main() { let _ = DocumentPixelStore::default(); }
```

```compile_fail
use photrez_core::pixel::PixelStoreRegistry;
fn main() {
    let mut store = PixelStoreRegistry::new();
    let _ = store.get_layer_mut("doc", "layer");
}
```
"#
)]
#[derive(Default)]
pub struct PixelStoreRegistry {
    #[cfg(feature = "legacy-host")]
    pub docs: HashMap<String, DocumentPixelStore>,
    #[cfg(not(feature = "legacy-host"))]
    pub(crate) docs: HashMap<String, DocumentPixelStore>,
}

impl PixelStoreRegistry {
    pub fn new() -> Self {
        Self {
            docs: HashMap::new(),
        }
    }
}

// Process-lifetime registry. The `static PIXEL_STORE: Option<HashMap<..>>`
// global is replaced here by a document-namespaced owner; the outer `Option`
// keeps the lazy-init pattern already used by the Tauri command layer.
#[cfg(any(feature = "legacy-host", test))]
static REGISTRY: Mutex<Option<PixelStoreRegistry>> = Mutex::new(None);

#[cfg(feature = "legacy-host")]
pub fn registry() -> MutexGuard<'static, Option<PixelStoreRegistry>> {
    REGISTRY.lock().unwrap()
}

#[cfg(all(not(feature = "legacy-host"), test))]
pub(crate) fn registry() -> MutexGuard<'static, Option<PixelStoreRegistry>> {
    REGISTRY.lock().unwrap()
}

mod access;
mod history;
mod pixels;

pub use pixels::CommitPhases;

#[cfg(test)]
mod tests;
