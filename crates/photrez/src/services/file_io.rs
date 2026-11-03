use super::error::ServiceError;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

#[cfg(unix)]
#[path = "file_io/unix_read.rs"]
mod platform_read;
#[cfg(windows)]
#[path = "file_io/windows_read.rs"]
mod platform_read;
#[cfg(not(any(unix, windows)))]
mod platform_read {
    use super::ServiceError;
    use std::fs::File;
    use std::path::Path;

    pub(super) fn open_read_file(_path: &Path) -> Result<File, ServiceError> {
        Err(ServiceError::Validation(
            "Handle-bound reads are unsupported on this platform".to_owned(),
        ))
    }
}

static NEXT_TEMP_FILE_ID: AtomicU64 = AtomicU64::new(1);
const TEMP_FILE_ATTEMPTS: usize = 16;

/// An authorized, canonical `.ptz` destination. Callers cannot construct or
/// modify a target; only `FileIo::authorize_save_target` can issue one.
#[derive(Debug)]
pub struct SaveTarget {
    path: PathBuf,
}

impl SaveTarget {
    /// Read-only view for the service that consumes the authorized target.
    pub fn as_path(&self) -> &Path {
        &self.path
    }
}

/// File path approval and save-target authorization boundary.
pub trait FileIo: Send + Sync {
    /// Persist an explicit approval, normally following a user-selected path.
    fn approve_path(&self, raw_path: &Path) -> Result<(), ServiceError>;

    /// Read a bounded file only when its exact path has been approved.
    fn read_approved(&self, raw_path: &Path, max_bytes: u64) -> Result<Vec<u8>, ServiceError>;

    /// Return a file's last-modified time as Unix milliseconds, only when its
    /// exact path has been approved. Startup recovery uses it to tell an archive
    /// that predates the file on screen from one that holds newer work.
    fn modified_ms(&self, raw_path: &Path) -> Result<u64, ServiceError>;

    /// Return an opaque target only when the normalized path is trusted.
    fn authorize_save_target(&self, raw_path: &Path) -> Result<SaveTarget, ServiceError>;
}

/// Filesystem-backed approval store used by the native application.
#[derive(Debug)]
pub struct LocalFileIo {
    approved_paths: Mutex<Vec<PathBuf>>,
    cache_root: PathBuf,
    persistence_path: PathBuf,
}

impl LocalFileIo {
    /// Open persisted approvals without creating or repairing the persistence
    /// file. Missing persistence means an empty approval set; malformed data
    /// and other read failures are returned as typed service errors.
    pub fn new(
        cache_root: impl AsRef<Path>,
        persistence_path: impl AsRef<Path>,
    ) -> Result<Self, ServiceError> {
        let cache_root = canonical_safe_path(cache_root.as_ref())?;
        let persistence_path = canonical_safe_path(persistence_path.as_ref())?;
        let persisted = match fs::read_to_string(&persistence_path) {
            Ok(contents) => Some(contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(ServiceError::Io(error)),
        };

        let mut approved_paths: Vec<PathBuf> = Vec::new();
        if let Some(contents) = persisted {
            let paths: Vec<String> = serde_json::from_str(&contents).map_err(|error| {
                ServiceError::Validation(format!("Malformed path approval data: {error}"))
            })?;
            for value in paths {
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    return Err(ServiceError::Validation(
                        "Persisted approved paths must be absolute".to_owned(),
                    ));
                }
                let path = canonical_safe_path(&path)?;
                if !approved_paths
                    .iter()
                    .any(|approved| same_path(approved, &path))
                {
                    approved_paths.push(path);
                }
            }
        }

        Ok(Self {
            approved_paths: Mutex::new(approved_paths),
            cache_root,
            persistence_path,
        })
    }

    fn persist_approvals(&self, paths: &[PathBuf]) -> Result<(), ServiceError> {
        let parent = self.persistence_path.parent().ok_or_else(|| {
            ServiceError::Validation("Approval persistence path has no parent".to_owned())
        })?;
        fs::create_dir_all(parent).map_err(ServiceError::Io)?;

        let mut values = Vec::with_capacity(paths.len());
        for path in paths {
            let value = path.to_str().ok_or_else(|| {
                ServiceError::Validation(
                    "Approved paths must be representable as UTF-8 for persistence".to_owned(),
                )
            })?;
            values.push(value.to_owned());
        }
        values.sort();
        let bytes = serde_json::to_vec(&values)
            .map_err(|error| ServiceError::Internal(format!("Serialize approvals: {error}")))?;

        let (temp_path, mut file) = create_temp_file(&self.persistence_path)?;
        let write_result = file.write_all(&bytes).and_then(|()| file.sync_all());
        drop(file);
        if let Err(error) = write_result {
            remove_temp_file(&temp_path)?;
            return Err(ServiceError::Io(error));
        }
        if let Err(error) = fs::rename(&temp_path, &self.persistence_path) {
            remove_temp_file(&temp_path)?;
            return Err(ServiceError::Io(error));
        }
        Ok(())
    }
}

impl FileIo for LocalFileIo {
    fn approve_path(&self, raw_path: &Path) -> Result<(), ServiceError> {
        let path = canonical_safe_path(raw_path)?;
        let mut approved = self
            .approved_paths
            .lock()
            .map_err(|_| ServiceError::Internal("Path approval lock was poisoned".to_owned()))?;
        if approved.iter().any(|existing| same_path(existing, &path)) {
            return Ok(());
        }

        let mut next = approved.clone();
        next.push(path);
        self.persist_approvals(&next)?;
        *approved = next;
        Ok(())
    }

    fn read_approved(&self, raw_path: &Path, max_bytes: u64) -> Result<Vec<u8>, ServiceError> {
        let path = canonical_safe_path(raw_path)?;
        {
            let approved = self.approved_paths.lock().map_err(|_| {
                ServiceError::Internal("Path approval lock was poisoned".to_owned())
            })?;
            if !approved.iter().any(|existing| same_path(existing, &path)) {
                return Err(ServiceError::Validation(
                    "Read path has not been explicitly approved".to_owned(),
                ));
            }
        }

        let file = platform_read::open_read_file(&path)?;
        read_bounded_file(file, max_bytes)
    }

    fn modified_ms(&self, raw_path: &Path) -> Result<u64, ServiceError> {
        let path = canonical_safe_path(raw_path)?;
        {
            let approved = self.approved_paths.lock().map_err(|_| {
                ServiceError::Internal("Path approval lock was poisoned".to_owned())
            })?;
            if !approved.iter().any(|existing| same_path(existing, &path)) {
                return Err(ServiceError::Validation(
                    "Timestamp path has not been explicitly approved".to_owned(),
                ));
            }
        }

        let modified = fs::metadata(&path)
            .map_err(ServiceError::Io)?
            .modified()
            .map_err(ServiceError::Io)?;
        modified
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as u64)
            .map_err(|_| ServiceError::Validation("File timestamp predates the epoch".to_owned()))
    }

    fn authorize_save_target(&self, raw_path: &Path) -> Result<SaveTarget, ServiceError> {
        validate_ptz_extension(raw_path)?;
        let path = canonical_safe_path(raw_path)?;
        let approved = self
            .approved_paths
            .lock()
            .map_err(|_| ServiceError::Internal("Path approval lock was poisoned".to_owned()))?;
        if !is_at_or_below(&path, &self.cache_root)
            && !approved.iter().any(|existing| same_path(existing, &path))
        {
            return Err(ServiceError::Validation(
                "Save path has not been explicitly approved".to_owned(),
            ));
        }
        Ok(SaveTarget { path })
    }
}

fn read_bounded_file(file: File, max_bytes: u64) -> Result<Vec<u8>, ServiceError> {
    let file_metadata = file.metadata().map_err(ServiceError::Io)?;
    if !file_metadata.is_file() {
        return Err(ServiceError::Validation(
            "Read path must be a regular file".to_owned(),
        ));
    }
    if file_metadata.len() > max_bytes {
        return Err(ServiceError::ResourceLimit(
            "Read exceeds the maximum byte limit".to_owned(),
        ));
    }

    let capacity = usize::try_from(file_metadata.len())
        .map_err(|_| ServiceError::ResourceLimit("Read size does not fit in memory".to_owned()))?;
    let mut bytes = Vec::with_capacity(capacity);
    let mut reader = file.take(max_bytes.saturating_add(1));
    reader.read_to_end(&mut bytes).map_err(ServiceError::Io)?;
    let bytes_read = u64::try_from(bytes.len()).map_err(|_| {
        ServiceError::ResourceLimit("Read size does not fit in file-size range".to_owned())
    })?;
    if bytes_read > max_bytes {
        return Err(ServiceError::ResourceLimit(
            "Read exceeds the maximum byte limit".to_owned(),
        ));
    }
    Ok(bytes)
}

fn validate_ptz_extension(path: &Path) -> Result<(), ServiceError> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("ptz") => Ok(()),
        _ => Err(ServiceError::Validation(
            "Save path must use the .ptz extension".to_owned(),
        )),
    }
}

/// Lexically normalize and canonicalize the longest existing prefix for both
/// existing and not-yet-created targets. These pathname checks normalize
/// approval identity; read-time race resistance comes only from `platform_read`.
fn canonical_safe_path(raw: &Path) -> Result<PathBuf, ServiceError> {
    if raw.as_os_str().is_empty() || raw.as_os_str().to_string_lossy().contains('\0') {
        return Err(ServiceError::Validation(
            "Path is empty or invalid".to_owned(),
        ));
    }
    let absolute = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir().map_err(ServiceError::Io)?.join(raw)
    };

    let mut lexical = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(_) => lexical.push(component.as_os_str()),
            Component::RootDir => lexical.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if lexical.file_name().is_some() {
                    lexical.pop();
                }
            }
            Component::Normal(name) => {
                lexical.push(name);
                match fs::symlink_metadata(&lexical) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        return Err(ServiceError::Validation(
                            "Path contains a symbolic link".to_owned(),
                        ));
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(ServiceError::Io(error)),
                }
            }
        }
    }
    canonicalize_existing_prefix(&lexical)
}

fn canonicalize_existing_prefix(path: &Path) -> Result<PathBuf, ServiceError> {
    let mut existing = path.to_path_buf();
    let mut missing = Vec::<OsString>::new();
    loop {
        match fs::symlink_metadata(&existing) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(ServiceError::Validation(
                        "Path contains a symbolic link".to_owned(),
                    ));
                }
                let mut canonical = fs::canonicalize(&existing).map_err(ServiceError::Io)?;
                for component in missing.iter().rev() {
                    canonical.push(component);
                }
                return Ok(canonical);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let name = existing.file_name().ok_or_else(|| {
                    ServiceError::Validation("Path has no existing ancestor".to_owned())
                })?;
                missing.push(name.to_os_string());
                if !existing.pop() {
                    return Err(ServiceError::Validation(
                        "Path has no existing ancestor".to_owned(),
                    ));
                }
            }
            Err(error) => return Err(ServiceError::Io(error)),
        }
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    let mut left = left.components();
    let mut right = right.components();
    loop {
        match (left.next(), right.next()) {
            (None, None) => return true,
            (Some(left), Some(right)) if same_component(left, right) => {}
            _ => return false,
        }
    }
}

fn is_at_or_below(path: &Path, root: &Path) -> bool {
    let mut path_components = path.components();
    for root_component in root.components() {
        match path_components.next() {
            Some(path_component) if same_component(path_component, root_component) => {}
            _ => return false,
        }
    }
    true
}

fn same_component(left: Component<'_>, right: Component<'_>) -> bool {
    #[cfg(windows)]
    {
        let left = left.as_os_str().to_string_lossy();
        let right = right.as_os_str().to_string_lossy();
        left.eq_ignore_ascii_case(right.as_ref())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

fn create_temp_file(destination: &Path) -> Result<(PathBuf, File), ServiceError> {
    let parent = destination
        .parent()
        .ok_or_else(|| ServiceError::Validation("Destination path has no parent".to_owned()))?;
    let filename = destination
        .file_name()
        .ok_or_else(|| ServiceError::Validation("Destination path has no file name".to_owned()))?;

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
        "Could not allocate a unique approval temp file".to_owned(),
    ))
}

fn remove_temp_file(path: &Path) -> Result<(), ServiceError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ServiceError::Io(error)),
    }
}

#[cfg(test)]
mod tests;
