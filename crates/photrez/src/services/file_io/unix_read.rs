#![allow(unsafe_code)] // SAFETY: all unsafe operations here are limited to openat and ownership of returned descriptors.

use super::ServiceError;
use std::fs::File;
use std::path::Path;

// The flag bits and errno values below are the numbers each platform's own
// header gives its x86_64 (and, for macOS, arm64) ABI. A target without a
// verified `abi` module fails closed at the bottom of this file rather than
// borrowing another platform's numbers.

// Linux x86_64 UAPI: asm-generic/fcntl.h flag bits, asm-generic/errno.h ELOOP
// and ENOTDIR.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod abi {
    pub(super) const O_CLOEXEC: i32 = 0x0008_0000;
    pub(super) const O_DIRECTORY: i32 = 0x0001_0000;
    pub(super) const O_NOFOLLOW: i32 = 0x0002_0000;
    pub(super) const O_NONBLOCK: i32 = 0x0000_0800;
    pub(super) const ELOOP: i32 = 40;
    pub(super) const ENOTDIR: i32 = 20;
}

// macOS (xnu) bsd/sys/fcntl.h: O_NONBLOCK 0x4, O_NOFOLLOW 0x100,
// O_DIRECTORY 0x100000, O_CLOEXEC 0x1000000; bsd/sys/errno.h: ELOOP 62,
// ENOTDIR 20. open(2) names ELOOP as the O_NOFOLLOW-on-symlink error and
// ENOTDIR as the O_DIRECTORY-on-non-directory error, so the same two values
// drive the same classification as on Linux.
#[cfg(all(
    target_os = "macos",
    any(target_arch = "aarch64", target_arch = "x86_64")
))]
mod abi {
    pub(super) const O_CLOEXEC: i32 = 0x0100_0000;
    pub(super) const O_DIRECTORY: i32 = 0x0010_0000;
    pub(super) const O_NOFOLLOW: i32 = 0x0000_0100;
    pub(super) const O_NONBLOCK: i32 = 0x0000_0004;
    pub(super) const ELOOP: i32 = 62;
    pub(super) const ENOTDIR: i32 = 20;
}

/// The targets with a verified openat ABI: Linux x86_64 and either macOS arch.
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(
        target_os = "macos",
        any(target_arch = "aarch64", target_arch = "x86_64")
    )
))]
mod supported {
    use std::ffi::{CString, OsStr};
    use std::io;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Component, Path};

    use super::abi::{ELOOP, ENOTDIR, O_CLOEXEC, O_DIRECTORY, O_NOFOLLOW, O_NONBLOCK};
    use super::ServiceError;
    use std::fs::File;

    const DIRECTORY_FLAGS: i32 = O_CLOEXEC | O_DIRECTORY | O_NOFOLLOW;
    const FILE_FLAGS: i32 = O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK;

    // The C prototype is variadic; the mode argument is only read with O_CREAT,
    // which neither flag set below ever includes.
    unsafe extern "C" {
        fn openat(dirfd: i32, pathname: *const std::ffi::c_char, flags: i32, ...) -> i32;
    }

    pub(crate) struct DirectoryHandle(File);

    pub(crate) fn open_directory_path(path: &Path) -> Result<DirectoryHandle, ServiceError> {
        if !path.is_absolute() {
            return Err(ServiceError::Validation(
                "Read path is not an absolute path".to_owned(),
            ));
        }
        let mut current = DirectoryHandle(File::open("/").map_err(ServiceError::Io)?);
        for component in path.components() {
            match component {
                Component::RootDir => {}
                Component::Normal(name) => current = open_directory_at(&current, name)?,
                _ => {
                    return Err(ServiceError::Validation(
                        "Read path is not a normalized absolute path".to_owned(),
                    ));
                }
            }
        }
        Ok(current)
    }

    pub(crate) fn open_file_at(
        parent: &DirectoryHandle,
        name: &OsStr,
    ) -> Result<File, ServiceError> {
        validate_leaf(name)?;
        let name = CString::new(name.as_bytes()).map_err(|_| {
            ServiceError::Validation("Read path contains an invalid component".to_owned())
        })?;
        let fd = open_at(parent.0.as_raw_fd(), &name, FILE_FLAGS)?;
        // SAFETY: openat returned a new, exclusively owned descriptor; File takes ownership once.
        let file = unsafe { File::from_raw_fd(fd) };
        if !file.metadata().map_err(ServiceError::Io)?.is_file() {
            return Err(ServiceError::Validation(
                "Read path must be a regular file".to_owned(),
            ));
        }
        Ok(file)
    }

    pub(crate) fn open_read_file(path: &Path) -> Result<File, ServiceError> {
        let parent = path.parent().ok_or_else(|| {
            ServiceError::Validation("Read path has no parent directory".to_owned())
        })?;
        let name = path
            .file_name()
            .ok_or_else(|| ServiceError::Validation("Read path has no file name".to_owned()))?;
        open_file_at(&open_directory_path(parent)?, name)
    }

    fn open_directory_at(
        parent: &DirectoryHandle,
        name: &OsStr,
    ) -> Result<DirectoryHandle, ServiceError> {
        let name = CString::new(name.as_bytes()).map_err(|_| {
            ServiceError::Validation("Read path contains an invalid component".to_owned())
        })?;
        let fd = open_at(parent.0.as_raw_fd(), &name, DIRECTORY_FLAGS)?;
        // SAFETY: openat returned a new, exclusively owned descriptor; File takes ownership once.
        let directory = unsafe { File::from_raw_fd(fd) };
        if !directory.metadata().map_err(ServiceError::Io)?.is_dir() {
            return Err(ServiceError::Validation(
                "Read path ancestor must be a directory".to_owned(),
            ));
        }
        Ok(DirectoryHandle(directory))
    }

    fn open_at(dirfd: i32, name: &CString, flags: i32) -> Result<i32, ServiceError> {
        // SAFETY: dirfd is an open directory handle, name is NUL-terminated and alive for the call,
        // and flags omit O_CREAT so the variadic mode argument is not read.
        let fd = unsafe { openat(dirfd, name.as_ptr(), flags) };
        if fd >= 0 {
            return Ok(fd);
        }
        let error = io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(ELOOP) | Some(ENOTDIR)) {
            return Err(ServiceError::Validation(
                "Read path contains a symlink or non-directory ancestor".to_owned(),
            ));
        }
        Err(ServiceError::Io(error))
    }

    fn validate_leaf(name: &OsStr) -> Result<(), ServiceError> {
        let bytes = name.as_bytes();
        if bytes.is_empty() || bytes == b"." || bytes == b".." || bytes.contains(&b'/') {
            return Err(ServiceError::Validation(
                "Read path has an invalid file component".to_owned(),
            ));
        }
        Ok(())
    }
}

#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(
        target_os = "macos",
        any(target_arch = "aarch64", target_arch = "x86_64")
    )
))]
pub(super) use supported::open_read_file;

/// The production path opens by absolute path only; the parent-handle helpers
/// exist so the race-resistance tests can hold a directory handle across an
/// ancestor swap. They are re-exported for exactly that build, and only the two
/// functions the tests name - the handle type stays private to this module.
#[cfg(all(
    any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(
            target_os = "macos",
            any(target_arch = "aarch64", target_arch = "x86_64")
        )
    ),
    test
))]
pub(super) use supported::{open_directory_path, open_file_at};

#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(
        target_os = "macos",
        any(target_arch = "aarch64", target_arch = "x86_64")
    )
)))]
mod unsupported {
    use super::ServiceError;
    use std::fs::File;
    use std::path::Path;

    // Fail closed rather than guess ABI constants outside a verified target.
    pub(super) fn open_read_file(_path: &Path) -> Result<File, ServiceError> {
        Err(ServiceError::Validation(
            "Handle-bound reads are unsupported on this Unix platform".to_owned(),
        ))
    }
}

#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(
        target_os = "macos",
        any(target_arch = "aarch64", target_arch = "x86_64")
    )
)))]
pub(super) use unsupported::open_read_file;
