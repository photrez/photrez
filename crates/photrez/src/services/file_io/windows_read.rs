#![allow(unsafe_code)] // SAFETY: this module isolates NtCreateFile and handle-information FFI behind owned handles.

use super::ServiceError;
use std::ffi::{c_void, OsStr, OsString};
use std::fs::File;
use std::io;
use std::mem::{size_of, MaybeUninit};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Component, Path, Prefix};
use std::ptr;

type Handle = *mut c_void;
const OBJ_CASE_INSENSITIVE: u32 = 0x0000_0040;
const OBJ_DONT_REPARSE: u32 = 0x0000_1000;
const FILE_OPEN: u32 = 0x0000_0001;
const FILE_DIRECTORY_FILE: u32 = 0x0000_0001;
const FILE_NON_DIRECTORY_FILE: u32 = 0x0000_0040;
const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x0000_0020;
const FILE_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x0000_0080;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
const FILE_READ_DATA: u32 = 0x0000_0001;
const FILE_LIST_DIRECTORY: u32 = 0x0000_0001;
const FILE_TRAVERSE: u32 = 0x0000_0020;
const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
const SYNCHRONIZE: u32 = 0x0010_0000;
const FILE_SHARE_ALL: u32 = 0x0000_0007;
const FILE_ATTRIBUTE_TAG_INFO_CLASS: i32 = 9;
const STATUS_REPARSE_POINT_ENCOUNTERED: i32 = 0xC000_050B_u32 as i32;
const STATUS_STOPPED_ON_SYMLINK: i32 = 0x8000_002D_u32 as i32;
const STATUS_FILE_IS_A_DIRECTORY: i32 = 0xC000_00BA_u32 as i32;

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}

#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: Handle,
    object_name: *mut UnicodeString,
    attributes: u32,
    security_descriptor: *mut c_void,
    security_quality_of_service: *mut c_void,
}

#[repr(C)]
struct IoStatusBlock {
    _status: usize,
    _information: usize,
}

#[repr(C)]
#[derive(Default)]
struct FileAttributeTagInfo {
    file_attributes: u32,
    reparse_tag: u32,
}

#[repr(C)]
#[derive(Default)]
struct FileStandardInfo {
    allocation_size: i64,
    end_of_file: i64,
    number_of_links: u32,
    delete_pending: u8,
    directory: u8,
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtCreateFile(
        file_handle: *mut Handle,
        desired_access: u32,
        object_attributes: *const ObjectAttributes,
        io_status_block: *mut IoStatusBlock,
        allocation_size: *const i64,
        file_attributes: u32,
        share_access: u32,
        create_disposition: u32,
        create_options: u32,
        ea_buffer: *const c_void,
        ea_length: u32,
    ) -> i32;
    fn RtlNtStatusToDosError(status: i32) -> u32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetFileInformationByHandleEx(
        file: Handle,
        information_class: i32,
        information: *mut c_void,
        buffer_size: u32,
    ) -> i32;
}

pub(super) struct DirectoryHandle(OwnedHandle);

// The drive/UNC root is opened once with OBJ_DONT_REPARSE; descendants are
// always opened relative to that handle. DOS-device and UNC-provider root
// resolution itself is performed by the Windows I/O manager.
pub(super) fn open_directory_path(path: &Path) -> Result<DirectoryHandle, ServiceError> {
    let (root_path, names) = root_and_components(path)?;
    let mut current = open_nt(None, &root_path, true)?;
    for name in names {
        current = open_directory_at(&current, &name)?;
    }
    Ok(DirectoryHandle(current))
}

pub(super) fn open_file_at(parent: &DirectoryHandle, name: &OsStr) -> Result<File, ServiceError> {
    validate_leaf(name)?;
    let handle = open_nt(Some(parent.0.as_raw_handle()), name, false)?;
    Ok(File::from(handle))
}

pub(super) fn open_read_file(path: &Path) -> Result<File, ServiceError> {
    let parent = path
        .parent()
        .ok_or_else(|| ServiceError::Validation("Read path has no parent directory".to_owned()))?;
    let name = path
        .file_name()
        .ok_or_else(|| ServiceError::Validation("Read path has no file name".to_owned()))?;
    open_file_at(&open_directory_path(parent)?, name)
}

fn open_directory_at(parent: &OwnedHandle, name: &OsStr) -> Result<OwnedHandle, ServiceError> {
    validate_leaf(name)?;
    open_nt(Some(parent.as_raw_handle()), name, true)
}

fn open_nt(
    root: Option<Handle>,
    name: &OsStr,
    directory: bool,
) -> Result<OwnedHandle, ServiceError> {
    let mut wide: Vec<u16> = name.encode_wide().collect();
    let byte_length = wide
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|length| u16::try_from(length).ok())
        .filter(|length| *length != 0)
        .ok_or_else(|| ServiceError::Validation("Read path component is too long".to_owned()))?;
    if wide.contains(&0) {
        return Err(ServiceError::Validation(
            "Read path contains an invalid component".to_owned(),
        ));
    }

    let mut unicode_name = UnicodeString {
        length: byte_length,
        maximum_length: byte_length,
        buffer: wide.as_mut_ptr(),
    };
    let attributes = ObjectAttributes {
        length: size_of::<ObjectAttributes>() as u32,
        root_directory: root.unwrap_or(ptr::null_mut()),
        object_name: &mut unicode_name,
        attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
        security_descriptor: ptr::null_mut(),
        security_quality_of_service: ptr::null_mut(),
    };
    let mut output = ptr::null_mut();
    let mut status_block = MaybeUninit::<IoStatusBlock>::zeroed();
    let create_options = FILE_OPEN_REPARSE_POINT
        | FILE_SYNCHRONOUS_IO_NONALERT
        | if directory {
            FILE_DIRECTORY_FILE
        } else {
            FILE_NON_DIRECTORY_FILE
        };
    let desired_access = if directory {
        FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE
    } else {
        FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE
    };

    // SAFETY: all pointers refer to live stack/UTF-16 storage for this call; RootDirectory is an
    // owned open handle or null for the absolute root, and NtCreateFile retains no caller pointer.
    let status = unsafe {
        NtCreateFile(
            &mut output,
            desired_access,
            &attributes,
            status_block.as_mut_ptr(),
            ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_ALL,
            FILE_OPEN,
            create_options,
            ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(nt_status_error(status));
    }
    if output.is_null() {
        return Err(ServiceError::Internal(
            "NtCreateFile returned an empty handle".to_owned(),
        ));
    }
    // SAFETY: NtCreateFile succeeded and returned a unique live kernel handle; OwnedHandle closes it.
    let handle = unsafe { OwnedHandle::from_raw_handle(output) };
    verify_handle(&handle, directory)?;
    Ok(handle)
}

fn verify_handle(handle: &OwnedHandle, directory: bool) -> Result<(), ServiceError> {
    let raw = handle.as_raw_handle();
    let mut attributes = FileAttributeTagInfo::default();
    // SAFETY: raw is a live owned handle and attributes points to a writable, correctly sized struct.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            raw,
            FILE_ATTRIBUTE_TAG_INFO_CLASS,
            &mut attributes as *mut _ as *mut c_void,
            size_of::<FileAttributeTagInfo>() as u32,
        )
    };
    if ok == 0 {
        return Err(ServiceError::Io(io::Error::last_os_error()));
    }
    if attributes.file_attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(ServiceError::Validation(
            "Read path contains a reparse point".to_owned(),
        ));
    }

    let mut standard = FileStandardInfo::default();
    // SAFETY: raw is a live owned handle and standard points to a writable, correctly sized struct.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            raw,
            1,
            &mut standard as *mut _ as *mut c_void,
            size_of::<FileStandardInfo>() as u32,
        )
    };
    if ok == 0 {
        return Err(ServiceError::Io(io::Error::last_os_error()));
    }
    if (standard.directory != 0) != directory {
        return Err(ServiceError::Validation(if directory {
            "Read path ancestor must be a directory".to_owned()
        } else {
            "Read path must be a regular file".to_owned()
        }));
    }
    Ok(())
}

fn nt_status_error(status: i32) -> ServiceError {
    if status == STATUS_REPARSE_POINT_ENCOUNTERED || status == STATUS_STOPPED_ON_SYMLINK {
        return ServiceError::Validation("Read path contains a reparse point".to_owned());
    }
    if status == STATUS_FILE_IS_A_DIRECTORY {
        return ServiceError::Validation("Read path must be a regular file".to_owned());
    }
    // SAFETY: RtlNtStatusToDosError is a pure conversion of the NTSTATUS value and has no pointer args.
    let win32_error = unsafe { RtlNtStatusToDosError(status) };
    ServiceError::Io(io::Error::from_raw_os_error(win32_error as i32))
}

fn root_and_components(path: &Path) -> Result<(OsString, Vec<OsString>), ServiceError> {
    let mut components = path.components();
    let prefix = match components.next() {
        Some(Component::Prefix(prefix)) => prefix.kind(),
        _ => {
            return Err(ServiceError::Validation(
                "Read path must be an absolute Windows path".to_owned(),
            ));
        }
    };
    let root = match prefix {
        Prefix::Disk(disk) | Prefix::VerbatimDisk(disk) => {
            let mut root = OsString::from(r"\??\");
            root.push(char::from(disk).to_string());
            root.push(":");
            root.push("\u{5c}");
            root
        }
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
            let mut root = OsString::from(r"\??\UNC\");
            root.push(server);
            root.push("\u{5c}");
            root.push(share);
            root.push("\u{5c}");
            root
        }
        Prefix::Verbatim(verbatim) => {
            let verbatim = verbatim.to_string_lossy();
            let Some(rest) = verbatim.strip_prefix(r"\\?\") else {
                return Err(ServiceError::Validation(
                    "Read path has an unsupported Windows prefix".to_owned(),
                ));
            };
            let mut root = OsString::from(r"\??\");
            root.push(rest);
            root.push("\u{5c}");
            root
        }
        _ => {
            return Err(ServiceError::Validation(
                "Read path has an unsupported Windows prefix".to_owned(),
            ));
        }
    };
    if !matches!(components.next(), Some(Component::RootDir)) {
        return Err(ServiceError::Validation(
            "Read path must have a root directory".to_owned(),
        ));
    }
    let mut names = Vec::new();
    for component in components {
        match component {
            Component::Normal(name) => names.push(name.to_os_string()),
            _ => {
                return Err(ServiceError::Validation(
                    "Read path is not a normalized absolute path".to_owned(),
                ));
            }
        }
    }
    Ok((root, names))
}

fn validate_leaf(name: &OsStr) -> Result<(), ServiceError> {
    let mut components = Path::new(name).components();
    if matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none() {
        Ok(())
    } else {
        Err(ServiceError::Validation(
            "Read path has an invalid file component".to_owned(),
        ))
    }
}
