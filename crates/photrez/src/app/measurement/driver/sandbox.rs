//! The measurement run's scratch directory: a unique path under the system temp
//! dir, spelled without a link so the file boundary accepts it, deleted when the
//! run finishes.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct Sandbox {
    pub(super) root: PathBuf,
}

impl Sandbox {
    pub(super) fn create() -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock precedes Unix epoch: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "photrez-measure-input-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&root)
            .map_err(|error| format!("cannot create sandbox {}: {error}", root.display()))?;
        // Link-free spelling: the file boundary rejects a path behind a link.
        Ok(Self {
            root: std::fs::canonicalize(&root).unwrap_or(root),
        })
    }

    pub(super) fn cleanup(&self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
