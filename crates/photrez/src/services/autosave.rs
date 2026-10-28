use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::services::error::ServiceError;
use crate::services::file_io::FileIo;
use crate::services::ptz_limits;
use crate::services::save_stream::SaveStream;
use crate::services::session::DocumentSession;

const MANIFEST_NAME: &str = "manifest.json";
const STALE_AGE_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const TEMP_FILE_ATTEMPTS: usize = 16;
static NEXT_TEMP_FILE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AutosaveEntry {
    doc_id: String,
    display_name: String,
    unix_ms: Option<u64>,
}

impl AutosaveEntry {
    pub(crate) fn doc_id(&self) -> &str {
        &self.doc_id
    }

    pub(crate) fn display_name(&self) -> &str {
        &self.display_name
    }

    pub(crate) fn unix_ms(&self) -> Option<u64> {
        self.unix_ms
    }
}

#[derive(Default)]
pub(crate) struct AutosaveManifest {
    entries: BTreeMap<String, AutosaveEntry>,
}

impl AutosaveManifest {
    fn parse(bytes: &[u8]) -> Result<Self, ServiceError> {
        let values: BTreeMap<String, String> = serde_json::from_slice(bytes)
            .map_err(|error| validation(format!("invalid autosave manifest: {error}")))?;
        let mut entries = BTreeMap::new();
        for (doc_id, value) in values {
            ptz_limits::validate_layer_id(&doc_id)?;
            let (display_name, unix_ms) = match value.rsplit_once('|') {
                Some((display_name, timestamp)) => {
                    let unix_ms = timestamp.parse::<u64>().map_err(|error| {
                        validation(format!("invalid autosave timestamp for {doc_id}: {error}"))
                    })?;
                    (display_name.to_owned(), Some(unix_ms))
                }
                None => (value, None),
            };
            entries.insert(
                doc_id.clone(),
                AutosaveEntry {
                    doc_id,
                    display_name,
                    unix_ms,
                },
            );
        }
        Ok(Self { entries })
    }

    fn encode(&self) -> Result<Vec<u8>, ServiceError> {
        let values = self
            .entries
            .iter()
            .map(|(doc_id, entry)| {
                let value = match entry.unix_ms {
                    Some(unix_ms) => format!("{}|{unix_ms}", entry.display_name),
                    None => entry.display_name.clone(),
                };
                (doc_id.clone(), value)
            })
            .collect::<BTreeMap<_, _>>();
        serde_json::to_vec(&values).map_err(|error| {
            ServiceError::Internal(format!("serialize autosave manifest: {error}"))
        })
    }
}

pub(crate) struct AutosaveStore {
    cache_root: PathBuf,
    autosave_dir: PathBuf,
    manifest_path: PathBuf,
    #[cfg(test)]
    fail_before_manifest_rename: AtomicBool,
}

impl AutosaveStore {
    pub(crate) fn new(cache_root: impl AsRef<Path>) -> Self {
        let cache_root = cache_root.as_ref().to_path_buf();
        let autosave_dir = cache_root.join("autosave");
        let manifest_path = autosave_dir.join(MANIFEST_NAME);
        Self {
            cache_root,
            autosave_dir,
            manifest_path,
            #[cfg(test)]
            fail_before_manifest_rename: AtomicBool::new(false),
        }
    }

    pub(crate) fn autosave(
        &self,
        doc_id: &str,
        session: &DocumentSession,
        files: &dyn FileIo,
        save_stream: &SaveStream,
        unix_ms: u64,
    ) -> Result<bool, ServiceError> {
        ptz_limits::validate_layer_id(doc_id)?;
        let loaded = session.loaded_document();
        if session.doc_id() != doc_id || loaded.document_id() != doc_id {
            return Err(validation(
                "autosave document ID does not match the session",
            ));
        }
        if session.is_closed() {
            return Err(ServiceError::Conflict("Document session is closed".into()));
        }
        if !session.is_dirty() {
            return Ok(false);
        }

        let mut manifest = self.read_manifest()?;
        fs::create_dir_all(&self.autosave_dir).map_err(ServiceError::Io)?;
        let path = self.archive_path(doc_id)?;
        loaded.save_to(files, &path, save_stream)?;
        manifest.entries.insert(
            doc_id.to_owned(),
            AutosaveEntry {
                doc_id: doc_id.to_owned(),
                display_name: session.display_name().to_owned(),
                unix_ms: Some(unix_ms),
            },
        );
        self.write_manifest(&manifest)?;
        Ok(true)
    }

    pub(crate) fn list(&self) -> Result<Vec<AutosaveEntry>, ServiceError> {
        Ok(self.read_manifest()?.entries.into_values().collect())
    }

    pub(crate) fn recovery_path(&self, entry: &AutosaveEntry) -> Result<PathBuf, ServiceError> {
        let path = self.archive_path(entry.doc_id())?;
        if !path.starts_with(&self.cache_root) {
            return Err(validation("autosave recovery path escaped cache root"));
        }
        Ok(path)
    }

    pub(crate) fn clear_one(&self, doc_id: &str) -> Result<bool, ServiceError> {
        ptz_limits::validate_layer_id(doc_id)?;
        let mut manifest = self.read_manifest()?;
        if !manifest.entries.contains_key(doc_id) {
            return Ok(false);
        }
        remove_archive(&self.archive_path(doc_id)?)?;
        manifest.entries.remove(doc_id);
        self.write_manifest(&manifest)?;
        Ok(true)
    }

    pub(crate) fn clear_all(&self) -> Result<usize, ServiceError> {
        let mut manifest = self.read_manifest()?;
        let mut removed = Vec::new();
        let mut first_error = None;
        for doc_id in manifest.entries.keys() {
            match remove_archive(&self.archive_path(doc_id)?) {
                Ok(()) => removed.push(doc_id.clone()),
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        for doc_id in &removed {
            manifest.entries.remove(doc_id);
        }
        if !removed.is_empty() {
            self.write_manifest(&manifest)?;
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(removed.len())
    }

    pub(crate) fn cleanup_stale(&self, now_ms: u64) -> Result<usize, ServiceError> {
        let mut manifest = self.read_manifest()?;
        let stale = manifest
            .entries
            .iter()
            .filter_map(|(doc_id, entry)| {
                entry
                    .unix_ms
                    .filter(|saved_at| now_ms.saturating_sub(*saved_at) > STALE_AGE_MS)
                    .map(|_| doc_id.clone())
            })
            .collect::<Vec<_>>();
        let mut removed = Vec::new();
        let mut first_error = None;
        for doc_id in stale {
            match remove_archive(&self.archive_path(&doc_id)?) {
                Ok(()) => removed.push(doc_id),
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        for doc_id in &removed {
            manifest.entries.remove(doc_id);
        }
        if !removed.is_empty() {
            self.write_manifest(&manifest)?;
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(removed.len())
    }

    fn archive_path(&self, doc_id: &str) -> Result<PathBuf, ServiceError> {
        ptz_limits::validate_layer_id(doc_id)?;
        Ok(self.autosave_dir.join(format!("{doc_id}.ptz")))
    }

    fn read_manifest(&self) -> Result<AutosaveManifest, ServiceError> {
        match fs::read(&self.manifest_path) {
            Ok(bytes) => AutosaveManifest::parse(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(AutosaveManifest::default())
            }
            Err(error) => Err(ServiceError::Io(error)),
        }
    }

    fn write_manifest(&self, manifest: &AutosaveManifest) -> Result<(), ServiceError> {
        let bytes = manifest.encode()?;
        fs::create_dir_all(&self.autosave_dir).map_err(ServiceError::Io)?;
        let (temp_path, mut file) = self.create_temp_file()?;
        let write_result = file.write_all(&bytes).and_then(|()| file.sync_all());
        drop(file);
        if let Err(error) = write_result {
            remove_temp_file(&temp_path)?;
            return Err(ServiceError::Io(error));
        }
        #[cfg(test)]
        if self
            .fail_before_manifest_rename
            .swap(false, Ordering::Relaxed)
        {
            remove_temp_file(&temp_path)?;
            return Err(ServiceError::Io(std::io::Error::other(
                "injected manifest replacement failure",
            )));
        }
        if let Err(error) = fs::rename(&temp_path, &self.manifest_path) {
            remove_temp_file(&temp_path)?;
            return Err(ServiceError::Io(error));
        }
        Ok(())
    }

    fn create_temp_file(&self) -> Result<(PathBuf, File), ServiceError> {
        for _ in 0..TEMP_FILE_ATTEMPTS {
            let id = NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
            let path = self
                .autosave_dir
                .join(format!("{MANIFEST_NAME}.tmp-{}-{id}", std::process::id()));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok((path, file)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(ServiceError::Io(error)),
            }
        }
        Err(ServiceError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not allocate autosave manifest temp file",
        )))
    }

    #[cfg(test)]
    fn autosave_dir(&self) -> &Path {
        &self.autosave_dir
    }

    #[cfg(test)]
    fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    #[cfg(test)]
    fn fail_next_manifest_rename(&self) {
        self.fail_before_manifest_rename
            .store(true, Ordering::Relaxed);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SaveJob {
    Manual(String),
    Autosave(String),
}

#[derive(Default)]
pub(crate) struct SaveQueue {
    running: Option<SaveJob>,
    pending: Option<SaveJob>,
}

impl SaveQueue {
    pub(crate) fn enqueue_manual(&mut self, doc_id: impl Into<String>) -> bool {
        let job = SaveJob::Manual(doc_id.into());
        if self.running.is_none() {
            self.running = Some(job);
            return true;
        }
        if self.pending.is_some() {
            return false;
        }
        self.pending = Some(job);
        true
    }

    pub(crate) fn enqueue_autosave(&mut self, doc_id: impl Into<String>) -> bool {
        if self.running.is_some() || self.pending.is_some() {
            return false;
        }
        self.running = Some(SaveJob::Autosave(doc_id.into()));
        true
    }

    pub(crate) fn complete(&mut self) -> Option<SaveJob> {
        self.running.take()?;
        self.running = self.pending.take();
        self.running.clone()
    }

    pub(crate) fn pending(&self) -> Option<&SaveJob> {
        self.pending.as_ref()
    }
}

fn remove_archive(path: &Path) -> Result<(), ServiceError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ServiceError::Io(error)),
    }
}

fn remove_temp_file(path: &Path) -> Result<(), ServiceError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ServiceError::Io(error)),
    }
}

fn validation(message: impl Into<String>) -> ServiceError {
    ServiceError::Validation(message.into())
}

#[cfg(test)]
mod tests;
