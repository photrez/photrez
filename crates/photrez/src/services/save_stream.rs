use super::error::ServiceError;
use super::file_io::SaveTarget;
use super::ptz_limits::{self, ArchiveBudget};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use zip::write::{SimpleFileOptions, ZipWriter};

const TEMP_FILE_ATTEMPTS: usize = 16;

static NEXT_TEMP_FILE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_HANDLE_ID: AtomicU64 = AtomicU64::new(1);
// In-process path exclusion only; cross-process locking is intentionally out of scope.
static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

#[derive(Default)]
struct Registry {
    sessions: HashMap<SaveHandle, Session>,
    active_paths: Vec<PathBuf>,
}

enum SessionState {
    Writing(Box<ZipWriter<File>>),
    Finalizing,
    CleanupOnly,
}

struct Session {
    state: SessionState,
    temp_path: PathBuf,
    final_path: PathBuf,
    layer_ids: HashSet<String>,
    budget: ArchiveBudget,
    #[cfg(test)]
    fail_finalize_sync: bool,
    #[cfg(test)]
    fail_remove_attempts: usize,
}

/// Incremental PTZ archive writer.
pub struct SaveStream;

/// Opaque identifier for an active save session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SaveHandle(u64);

impl SaveStream {
    pub fn begin(
        &self,
        target: SaveTarget,
        document_json: &[u8],
    ) -> Result<SaveHandle, ServiceError> {
        let mut budget = ArchiveBudget::new();
        let document_size = u64::try_from(document_json.len())
            .map_err(|_| ServiceError::ResourceLimit("document size overflow".into()))?;
        budget.reserve_document_json(document_size)?;

        let final_path = target.as_path().to_path_buf();
        ensure_target_replaceable(&final_path)?;
        {
            let mut registry = registry();
            if registry
                .active_paths
                .iter()
                .any(|active| same_path(active, &final_path))
            {
                return Err(ServiceError::Conflict(
                    "A save session already uses this target".into(),
                ));
            }
            registry.active_paths.push(final_path.clone());
        }

        let setup = (|| {
            let (temp_path, file) = create_temp_file(&final_path)?;
            let mut writer = ZipWriter::new(file);
            let write_result = writer
                .start_file("document.json", file_options())
                .map_err(map_zip_error)
                .and_then(|()| writer.write_all(document_json).map_err(ServiceError::Io));
            if let Err(error) = write_result {
                drop(writer);
                return match remove_temp_file(&temp_path) {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => Err(cleanup_error),
                };
            }
            Ok((temp_path, writer))
        })();

        let (temp_path, writer) = match setup {
            Ok(setup) => setup,
            Err(error) => {
                release_path_lock(&final_path);
                return Err(error);
            }
        };

        let mut registry = registry();
        let handle = loop {
            let handle = SaveHandle(NEXT_HANDLE_ID.fetch_add(1, Ordering::Relaxed));
            if !registry.sessions.contains_key(&handle) {
                break handle;
            }
        };
        registry.sessions.insert(
            handle,
            Session {
                state: SessionState::Writing(Box::new(writer)),
                temp_path,
                final_path,
                layer_ids: HashSet::new(),
                budget,
                #[cfg(test)]
                fail_finalize_sync: false,
                #[cfg(test)]
                fail_remove_attempts: 0,
            },
        );
        Ok(handle)
    }

    pub fn write_layer(
        &self,
        handle: SaveHandle,
        layer_id: &str,
        png_bytes: &[u8],
    ) -> Result<(), ServiceError> {
        ptz_limits::validate_layer_id(layer_id)?;

        let mut registry = registry();
        let zip_result = {
            let session = registry
                .sessions
                .get_mut(&handle)
                .ok_or_else(|| ServiceError::Conflict("Save session is not active".into()))?;
            if !matches!(session.state, SessionState::Writing(_)) {
                return Err(ServiceError::Conflict(
                    "Save session is finalizing or requires cancellation".into(),
                ));
            }
            if session.layer_ids.contains(layer_id) {
                return Err(ServiceError::Validation(
                    "Layer IDs must be unique within a save".into(),
                ));
            }
            if !png_bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                return Err(ServiceError::Validation("Layer data is not a PNG".into()));
            }
            let dimensions = photrez_core::pixel::export::png_dimensions(png_bytes)
                .map_err(|error| ServiceError::Validation(format!("Invalid PNG: {error}")))?;
            let encoded_size = u64::try_from(png_bytes.len())
                .map_err(|_| ServiceError::ResourceLimit("PNG size overflow".into()))?;
            session.budget.reserve_layer(
                layer_id,
                u64::from(dimensions.width()),
                u64::from(dimensions.height()),
                encoded_size,
            )?;

            let entry_name = format!("layers/{layer_id}.png");
            let result = match &mut session.state {
                SessionState::Writing(writer) => writer
                    .start_file(entry_name, file_options())
                    .map_err(map_zip_error)
                    .and_then(|()| writer.write_all(png_bytes).map_err(ServiceError::Io)),
                SessionState::Finalizing | SessionState::CleanupOnly => {
                    return Err(ServiceError::Conflict(
                        "Save session is finalizing or requires cancellation".into(),
                    ));
                }
            };
            if result.is_ok() {
                session.layer_ids.insert(layer_id.to_owned());
            }
            result
        };

        if let Err(error) = zip_result {
            return match abort_session(&mut registry, handle) {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(cleanup_error),
            };
        }
        Ok(())
    }

    pub fn end(&self, handle: SaveHandle) -> Result<(), ServiceError> {
        let (writer, temp_path, final_path, fail_sync) = {
            let mut registry = registry();
            let session = registry
                .sessions
                .get_mut(&handle)
                .ok_or_else(|| ServiceError::Conflict("Save session is not active".into()))?;
            let writer = match std::mem::replace(&mut session.state, SessionState::Finalizing) {
                SessionState::Writing(writer) => writer,
                state @ (SessionState::Finalizing | SessionState::CleanupOnly) => {
                    session.state = state;
                    return Err(ServiceError::Conflict(
                        "Save session is already finalizing or requires cancellation".into(),
                    ));
                }
            };
            #[cfg(test)]
            let fail_sync = std::mem::replace(&mut session.fail_finalize_sync, false);
            #[cfg(not(test))]
            let fail_sync = false;
            (
                writer,
                session.temp_path.clone(),
                session.final_path.clone(),
                fail_sync,
            )
        };

        match finalize_to_disk(*writer, &temp_path, &final_path, fail_sync) {
            Ok(()) => {
                let mut registry = registry();
                registry.sessions.remove(&handle);
                release_path_lock_from_registry(&mut registry, &final_path);
                Ok(())
            }
            Err(error) => {
                if let Some(session) = registry().sessions.get_mut(&handle) {
                    session.state = SessionState::CleanupOnly;
                }
                Err(error)
            }
        }
    }

    pub fn cancel(&self, handle: SaveHandle) -> Result<(), ServiceError> {
        let mut registry = registry();
        let final_path = {
            let session = registry
                .sessions
                .get_mut(&handle)
                .ok_or_else(|| ServiceError::Conflict("Save session is not active".into()))?;
            match std::mem::replace(&mut session.state, SessionState::CleanupOnly) {
                SessionState::Writing(writer) => drop(writer),
                SessionState::CleanupOnly => {}
                SessionState::Finalizing => {
                    session.state = SessionState::Finalizing;
                    return Err(ServiceError::Conflict(
                        "Save session is still finalizing".into(),
                    ));
                }
            }
            remove_session_temp_file(session)?;
            session.final_path.clone()
        };
        registry.sessions.remove(&handle);
        release_path_lock_from_registry(&mut registry, &final_path);
        Ok(())
    }
}

fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY
        .get_or_init(|| Mutex::new(Registry::default()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn file_options() -> SimpleFileOptions {
    SimpleFileOptions::DEFAULT
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6))
        .system(zip::System::Dos)
        .external_attributes(0o600 << 16)
}

fn create_temp_file(destination: &Path) -> Result<(PathBuf, File), ServiceError> {
    let parent = destination
        .parent()
        .ok_or_else(|| ServiceError::Validation("Destination path has no parent".into()))?;
    let filename = destination
        .file_name()
        .ok_or_else(|| ServiceError::Validation("Destination path has no file name".into()))?;

    for _ in 0..TEMP_FILE_ATTEMPTS {
        let id = NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
        let mut temp_name = filename.to_os_string();
        temp_name.push(format!(".{}.{}.tmp", std::process::id(), id));
        let temp_path = parent.join(temp_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => return Ok((temp_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(ServiceError::Io(error)),
        }
    }
    Err(ServiceError::ResourceLimit(
        "Could not allocate a unique save temp file".into(),
    ))
}

fn ensure_target_replaceable(path: &Path) -> Result<(), ServiceError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(ServiceError::Validation(
            "Save target must not be a symbolic link".into(),
        )),
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(ServiceError::Validation(
            "Existing save target must be a regular file".into(),
        )),
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

fn abort_session(registry: &mut Registry, handle: SaveHandle) -> Result<(), ServiceError> {
    let final_path = {
        let session = registry
            .sessions
            .get_mut(&handle)
            .ok_or_else(|| ServiceError::Conflict("Save session is not active".into()))?;
        match std::mem::replace(&mut session.state, SessionState::CleanupOnly) {
            SessionState::Writing(writer) => drop(writer),
            SessionState::CleanupOnly => {}
            SessionState::Finalizing => {
                session.state = SessionState::Finalizing;
                return Err(ServiceError::Conflict(
                    "Save session is still finalizing".into(),
                ));
            }
        }
        remove_session_temp_file(session)?;
        session.final_path.clone()
    };
    registry.sessions.remove(&handle);
    release_path_lock_from_registry(registry, &final_path);
    Ok(())
}

fn remove_session_temp_file(session: &mut Session) -> Result<(), ServiceError> {
    #[cfg(test)]
    if session.fail_remove_attempts > 0 {
        session.fail_remove_attempts -= 1;
        return Err(ServiceError::Io(std::io::Error::other(
            "injected temp file removal failure",
        )));
    }
    remove_temp_file(&session.temp_path)
}

fn finalize_to_disk(
    writer: ZipWriter<File>,
    temp_path: &Path,
    final_path: &Path,
    fail_sync: bool,
) -> Result<(), ServiceError> {
    let file = writer.finish().map_err(map_zip_error)?;
    if fail_sync {
        drop(file);
        return Err(ServiceError::Io(std::io::Error::other(
            "injected finalize sync failure",
        )));
    }
    file.sync_all().map_err(ServiceError::Io)?;
    drop(file);
    sync_parent(final_path);
    ensure_target_replaceable(final_path)?;
    fs::rename(temp_path, final_path).map_err(ServiceError::Io)
}

fn release_path_lock(path: &Path) {
    release_path_lock_from_registry(&mut registry(), path);
}

fn release_path_lock_from_registry(registry: &mut Registry, path: &Path) {
    registry
        .active_paths
        .retain(|active| !same_path(active, path));
}

fn same_path(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

fn sync_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
    }
}

fn map_zip_error(error: zip::result::ZipError) -> ServiceError {
    match error {
        zip::result::ZipError::Io(error) => ServiceError::Io(error),
        error => ServiceError::Internal(format!("ZIP archive write failed: {error}")),
    }
}

#[cfg(test)]
mod tests;
