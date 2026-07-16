//! Windows atomic project publication through `MoveFileExW`.
#![allow(unsafe_code)]

use std::ffi::c_void;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

use super::{platform_error, PublicationMode, PublicationStage};
use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use windows::core::{Error as WindowsError, PCWSTR};
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_CALL_NOT_IMPLEMENTED, ERROR_DISK_FULL,
    ERROR_DISK_QUOTA_EXCEEDED, ERROR_FILENAME_EXCED_RANGE, ERROR_FILE_EXISTS, ERROR_FILE_NOT_FOUND,
    ERROR_HANDLE_DISK_FULL, ERROR_INVALID_NAME, ERROR_LOCK_VIOLATION, ERROR_NOT_ENOUGH_MEMORY,
    ERROR_NOT_ENOUGH_QUOTA, ERROR_NOT_SAME_DEVICE, ERROR_NOT_SUPPORTED, ERROR_OUTOFMEMORY,
    ERROR_PATH_NOT_FOUND, ERROR_SHARING_VIOLATION, ERROR_WRITE_PROTECT, HANDLE, WIN32_ERROR,
};
use windows::Win32::Storage::FileSystem::{
    FileIdInfo, GetFileInformationByHandleEx, MoveFileExW, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_ID_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, MOVEFILE_REPLACE_EXISTING,
    MOVEFILE_WRITE_THROUGH, MOVE_FILE_FLAGS,
};

/// Stable native identity used to prove ownership before candidate cleanup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FileIdentity {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

/// Captures the volume serial and full 128-bit identifier from an open file handle.
pub(super) fn identity_from_file(file: &File) -> io::Result<FileIdentity> {
    let mut information = FILE_ID_INFO::default();
    let information_size = u32::try_from(std::mem::size_of::<FILE_ID_INFO>())
        .expect("FILE_ID_INFO size fits in the Win32 API width");
    // SAFETY: The Rust file owns a live Windows handle for this call. `information` points to a
    // writable FILE_ID_INFO buffer whose exact size is passed to the API.
    unsafe {
        GetFileInformationByHandleEx(
            HANDLE(file.as_raw_handle()),
            FileIdInfo,
            std::ptr::addr_of_mut!(information).cast::<c_void>(),
            information_size,
        )
    }
    .map_err(windows_error_to_io)?;
    Ok(FileIdentity {
        volume_serial_number: information.VolumeSerialNumber,
        file_id: information.FileId.Identifier,
    })
}

/// Opens one directory entry without following a reparse point and captures its native identity.
pub(super) fn identity_from_path(path: &Path) -> io::Result<FileIdentity> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)?;
    identity_from_file(&file)
}

fn windows_error_to_io(source: WindowsError) -> io::Error {
    WIN32_ERROR::from_error(&source).map_or_else(
        || io::Error::other(source),
        |code| io::Error::from_raw_os_error(code.0 as i32),
    )
}

/// Atomically publishes one fully synchronized same-directory candidate.
pub(super) fn publish(candidate: &Path, destination: &Path, mode: PublicationMode) -> Result<()> {
    validate_siblings(candidate, destination).map_err(|source| {
        platform_error(
            source,
            PublicationStage::NativePublish,
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "project publication requires distinct absolute sibling paths",
            None,
            candidate,
            destination,
        )
    })?;
    let candidate_wide = encode_path(candidate).map_err(|source| {
        platform_error(
            source,
            PublicationStage::NativePublish,
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "project candidate path is not representable by the Windows filesystem API",
            None,
            candidate,
            destination,
        )
    })?;
    let destination_wide = encode_path(destination).map_err(|source| {
        platform_error(
            source,
            PublicationStage::NativePublish,
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "project destination path is not representable by the Windows filesystem API",
            None,
            candidate,
            destination,
        )
    })?;

    // SAFETY: Both vectors are live, NUL-terminated UTF-16 path buffers for the duration of this
    // call. Core guarantees distinct absolute same-directory paths and a closed candidate. The
    // selected flags request only same-volume move durability and optional authorized replacement.
    unsafe {
        MoveFileExW(
            PCWSTR::from_raw(candidate_wide.as_ptr()),
            PCWSTR::from_raw(destination_wide.as_ptr()),
            publication_flags(mode),
        )
    }
    .map_err(|source| native_error(source, candidate, destination))
}

/// Finishes one no-clobber publication that core revalidated after a visible failure.
pub(super) fn recover_visible_no_clobber(candidate: &Path, destination: &Path) -> Result<()> {
    validate_siblings(candidate, destination).map_err(|source| {
        platform_error(
            source,
            PublicationStage::ParentSyncAfterPublish,
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "project recovery requires distinct absolute sibling paths",
            None,
            candidate,
            destination,
        )
    })?;

    match fs::symlink_metadata(candidate) {
        Err(source) if source.kind() == io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(io_native_error(
                source,
                PublicationStage::CandidateUnlink,
                "Windows recovery candidate state could not be inspected",
                candidate,
                destination,
            ));
        }
        Ok(_) => {
            return Err(recovery_state_error(
                "Windows no-clobber recovery preserves an unexpected candidate entry",
                candidate,
                destination,
            ));
        }
    }

    let destination_metadata = fs::symlink_metadata(destination).map_err(|source| {
        io_native_error(
            source,
            PublicationStage::ParentSyncAfterPublish,
            "Windows recovery destination state could not be inspected",
            candidate,
            destination,
        )
    })?;
    if destination_metadata.file_type().is_symlink() || !destination_metadata.is_file() {
        return Err(recovery_state_error(
            "Windows recovery destination is not the revalidated regular file",
            candidate,
            destination,
        ));
    }

    OpenOptions::new()
        .read(true)
        .write(true)
        .open(destination)
        .and_then(|file| file.sync_all())
        .map_err(|source| {
            io_native_error(
                source,
                PublicationStage::ParentSyncAfterPublish,
                "Windows visible publication could not be synchronized on retry",
                candidate,
                destination,
            )
        })
}

fn validate_siblings(candidate: &Path, destination: &Path) -> io::Result<()> {
    if !candidate.is_absolute()
        || !destination.is_absolute()
        || candidate == destination
        || candidate.parent().is_none()
        || candidate.parent() != destination.parent()
        || candidate.file_name().is_none()
        || destination.file_name().is_none()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "publication paths are not distinct absolute siblings",
        ));
    }
    Ok(())
}

fn encode_path(path: &Path) -> io::Result<Vec<u16>> {
    let mut encoded = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if encoded.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows project path contains an interior NUL",
        ));
    }
    encoded.push(0);
    Ok(encoded)
}

const fn publication_flags(mode: PublicationMode) -> MOVE_FILE_FLAGS {
    match mode {
        PublicationMode::ReplaceExisting => {
            MOVE_FILE_FLAGS(MOVEFILE_REPLACE_EXISTING.0 | MOVEFILE_WRITE_THROUGH.0)
        }
        PublicationMode::NoClobber => MOVEFILE_WRITE_THROUGH,
    }
}

fn native_error(source: WindowsError, candidate: &Path, destination: &Path) -> Error {
    let code = WIN32_ERROR::from_error(&source);
    let native_code = code.map_or_else(
        || format!("hresult:0x{:08x}", source.code().0 as u32),
        |code| format!("win32:{}", code.0),
    );
    let (category, recoverability) = classify_windows_error(code);
    platform_error(
        source,
        PublicationStage::NativePublish,
        category,
        recoverability,
        "Windows atomic project publication failed",
        Some(native_code),
        candidate,
        destination,
    )
}

fn io_native_error(
    source: io::Error,
    stage: PublicationStage,
    message: &'static str,
    candidate: &Path,
    destination: &Path,
) -> Error {
    let code = source.raw_os_error().map(|code| WIN32_ERROR(code as u32));
    let native_code = code.map(|code| format!("win32:{}", code.0));
    let (category, recoverability) = classify_windows_error(code);
    platform_error(
        source,
        stage,
        category,
        recoverability,
        message,
        native_code,
        candidate,
        destination,
    )
}

fn recovery_state_error(message: &'static str, candidate: &Path, destination: &Path) -> Error {
    platform_error(
        io::Error::other(message),
        PublicationStage::CandidateUnlink,
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        message,
        None,
        candidate,
        destination,
    )
}

fn classify_windows_error(code: Option<WIN32_ERROR>) -> (ErrorCategory, Recoverability) {
    match code {
        Some(code) if code == ERROR_FILE_EXISTS || code == ERROR_ALREADY_EXISTS => {
            (ErrorCategory::Conflict, Recoverability::UserCorrectable)
        }
        Some(code) if code == ERROR_ACCESS_DENIED || code == ERROR_WRITE_PROTECT => (
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
        ),
        Some(code) if code == ERROR_FILE_NOT_FOUND || code == ERROR_PATH_NOT_FOUND => {
            (ErrorCategory::NotFound, Recoverability::UserCorrectable)
        }
        Some(code)
            if code == ERROR_DISK_FULL
                || code == ERROR_HANDLE_DISK_FULL
                || code == ERROR_DISK_QUOTA_EXCEEDED
                || code == ERROR_OUTOFMEMORY
                || code == ERROR_NOT_ENOUGH_MEMORY
                || code == ERROR_NOT_ENOUGH_QUOTA =>
        {
            (
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
            )
        }
        Some(code)
            if code == ERROR_NOT_SAME_DEVICE
                || code == ERROR_NOT_SUPPORTED
                || code == ERROR_CALL_NOT_IMPLEMENTED =>
        {
            (ErrorCategory::Unsupported, Recoverability::UserCorrectable)
        }
        Some(code) if code == ERROR_INVALID_NAME || code == ERROR_FILENAME_EXCED_RANGE => {
            (ErrorCategory::InvalidInput, Recoverability::UserCorrectable)
        }
        Some(code) if code == ERROR_SHARING_VIOLATION || code == ERROR_LOCK_VIOLATION => {
            (ErrorCategory::Unavailable, Recoverability::Retryable)
        }
        _ => (ErrorCategory::Unavailable, Recoverability::Retryable),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use windows::Win32::Storage::FileSystem::MOVEFILE_COPY_ALLOWED;

    use super::*;

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "superi-save-windows-{label}-{}-{}",
                std::process::id(),
                NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn flags_never_allow_copy_and_replace_only_when_authorized() {
        let replace = publication_flags(PublicationMode::ReplaceExisting);
        assert!(replace.contains(MOVEFILE_REPLACE_EXISTING));
        assert!(replace.contains(MOVEFILE_WRITE_THROUGH));
        assert!(!replace.contains(MOVEFILE_COPY_ALLOWED));

        let no_clobber = publication_flags(PublicationMode::NoClobber);
        assert!(!no_clobber.contains(MOVEFILE_REPLACE_EXISTING));
        assert!(no_clobber.contains(MOVEFILE_WRITE_THROUGH));
        assert!(!no_clobber.contains(MOVEFILE_COPY_ALLOWED));
    }

    #[test]
    fn path_encoding_preserves_wtf16_and_appends_one_terminator() {
        let units = [b'C' as u16, b':' as u16, b'\\' as u16, 0xd800, b'x' as u16];
        let path = PathBuf::from(OsString::from_wide(&units));

        let encoded = encode_path(&path).unwrap();

        assert_eq!(&encoded[..encoded.len() - 1], units.as_slice());
        assert_eq!(encoded.last(), Some(&0));
    }

    #[test]
    fn path_encoding_rejects_interior_nul() {
        let units = [b'C' as u16, b':' as u16, b'\\' as u16, 0, b'x' as u16];
        let path = PathBuf::from(OsString::from_wide(&units));

        assert_eq!(
            encode_path(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn visible_no_clobber_recovery_syncs_a_moved_destination() {
        let root = TempRoot::new("recover-missing");
        let candidate = root.0.join("candidate");
        let destination = root.0.join("destination");
        let unrelated = root.0.join("unrelated");
        fs::write(&destination, b"published").unwrap();
        fs::write(&unrelated, b"preserve").unwrap();

        recover_visible_no_clobber(&candidate, &destination).unwrap();

        assert!(!candidate.exists());
        assert_eq!(fs::read(destination).unwrap(), b"published");
        assert_eq!(fs::read(unrelated).unwrap(), b"preserve");
    }

    #[test]
    fn visible_no_clobber_recovery_preserves_an_unexpected_candidate() {
        let root = TempRoot::new("recover-unexpected");
        let candidate = root.0.join("candidate");
        let destination = root.0.join("destination");
        fs::write(&candidate, b"candidate").unwrap();
        fs::write(&destination, b"destination").unwrap();

        let error = recover_visible_no_clobber(&candidate, &destination).unwrap_err();

        assert_eq!(error.category(), ErrorCategory::Unavailable);
        assert_eq!(error.recoverability(), Recoverability::Retryable);
        assert_eq!(fs::read(candidate).unwrap(), b"candidate");
        assert_eq!(fs::read(destination).unwrap(), b"destination");
    }

    #[test]
    fn file_identity_matches_handles_and_hard_links_but_not_other_files() {
        let root = TempRoot::new("identity");
        let original = root.0.join("original");
        let hard_link = root.0.join("hard-link");
        let unrelated = root.0.join("unrelated");
        fs::write(&original, b"original").unwrap();
        fs::write(&unrelated, b"unrelated").unwrap();
        fs::hard_link(&original, &hard_link).unwrap();
        let file = fs::File::open(&original).unwrap();

        assert_eq!(
            identity_from_file(&file).unwrap(),
            identity_from_path(&original).unwrap()
        );
        assert_eq!(
            identity_from_path(&original).unwrap(),
            identity_from_path(&hard_link).unwrap()
        );
        assert_ne!(
            identity_from_path(&original).unwrap(),
            identity_from_path(&unrelated).unwrap()
        );
    }

    #[test]
    fn native_classification_retains_expected_public_categories() {
        assert_eq!(
            classify_windows_error(Some(ERROR_ALREADY_EXISTS)).0,
            ErrorCategory::Conflict
        );
        assert_eq!(
            classify_windows_error(Some(ERROR_NOT_SAME_DEVICE)).0,
            ErrorCategory::Unsupported
        );
        assert_eq!(
            classify_windows_error(Some(ERROR_DISK_FULL)).0,
            ErrorCategory::ResourceExhausted
        );
        assert_eq!(
            classify_windows_error(Some(ERROR_DISK_QUOTA_EXCEEDED)).0,
            ErrorCategory::ResourceExhausted
        );
        assert_eq!(
            classify_windows_error(Some(ERROR_SHARING_VIOLATION)).1,
            Recoverability::Retryable
        );
    }

    #[test]
    fn defensive_boundary_rejects_non_sibling_paths() {
        assert_eq!(
            validate_siblings(
                Path::new("relative-candidate"),
                Path::new("relative-destination")
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
