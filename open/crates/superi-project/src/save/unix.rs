//! Unix atomic project publication and directory durability.

use std::fs::{self, File, Metadata};
use std::io;
use std::path::Path;

use super::{platform_error, PublicationMode, PublicationStage};
use superi_core::error::{Error, ErrorCategory, Recoverability, Result};

/// Stable native identity used to prove ownership before candidate cleanup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FileIdentity {
    device: u64,
    inode: u64,
}

/// Captures native identity from an already-open file handle.
pub(super) fn identity_from_file(file: &File) -> io::Result<FileIdentity> {
    file.metadata()
        .map(|metadata| identity_from_metadata(&metadata))
}

/// Captures native identity for the directory entry itself without following symlinks.
pub(super) fn identity_from_path(path: &Path) -> io::Result<FileIdentity> {
    fs::symlink_metadata(path).map(|metadata| identity_from_metadata(&metadata))
}

fn identity_from_metadata(metadata: &Metadata) -> FileIdentity {
    use std::os::unix::fs::MetadataExt;

    FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    }
}

/// Atomically publishes one fully synchronized same-directory candidate.
pub(super) fn publish(candidate: &Path, destination: &Path, mode: PublicationMode) -> Result<()> {
    publish_with_post_sync_hook(candidate, destination, mode, || {})
}

fn publish_with_post_sync_hook<F>(
    candidate: &Path,
    destination: &Path,
    mode: PublicationMode,
    after_first_parent_sync: F,
) -> Result<()>
where
    F: FnOnce(),
{
    let parent = sibling_parent(candidate, destination).map_err(|source| {
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

    match mode {
        PublicationMode::ReplaceExisting => {
            fs::rename(candidate, destination).map_err(|source| {
                native_error(
                    source,
                    PublicationStage::NativePublish,
                    mode,
                    candidate,
                    destination,
                )
            })?;
            sync_parent(
                parent,
                PublicationStage::ParentSyncAfterPublish,
                mode,
                candidate,
                destination,
            )
        }
        PublicationMode::NoClobber => {
            fs::hard_link(candidate, destination).map_err(|source| {
                native_error(
                    source,
                    PublicationStage::NativePublish,
                    mode,
                    candidate,
                    destination,
                )
            })?;
            sync_parent(
                parent,
                PublicationStage::ParentSyncAfterPublish,
                mode,
                candidate,
                destination,
            )?;
            after_first_parent_sync();
            finish_visible_no_clobber(parent, candidate, destination)
        }
    }
}

/// Finishes one no-clobber publication that core revalidated after a visible failure.
pub(super) fn recover_visible_no_clobber(candidate: &Path, destination: &Path) -> Result<()> {
    let parent = sibling_parent(candidate, destination).map_err(|source| {
        platform_error(
            source,
            PublicationStage::CandidateUnlink,
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "project recovery requires distinct absolute sibling paths",
            None,
            candidate,
            destination,
        )
    })?;

    match recovery_candidate_state(candidate, destination).map_err(|source| {
        native_error(
            source,
            PublicationStage::CandidateUnlink,
            PublicationMode::NoClobber,
            candidate,
            destination,
        )
    })? {
        RecoveryCandidateState::Missing => sync_parent(
            parent,
            PublicationStage::ParentSyncAfterPublish,
            PublicationMode::NoClobber,
            candidate,
            destination,
        ),
        RecoveryCandidateState::OwnedLink => {
            sync_parent(
                parent,
                PublicationStage::ParentSyncAfterPublish,
                PublicationMode::NoClobber,
                candidate,
                destination,
            )?;
            finish_visible_no_clobber(parent, candidate, destination)
        }
    }
}

fn finish_visible_no_clobber(parent: &Path, candidate: &Path, destination: &Path) -> Result<()> {
    match recovery_candidate_state(candidate, destination).map_err(|source| {
        native_error(
            source,
            PublicationStage::CandidateUnlink,
            PublicationMode::NoClobber,
            candidate,
            destination,
        )
    })? {
        RecoveryCandidateState::Missing => {}
        RecoveryCandidateState::OwnedLink => {
            fs::remove_file(candidate).map_err(|source| {
                native_error(
                    source,
                    PublicationStage::CandidateUnlink,
                    PublicationMode::NoClobber,
                    candidate,
                    destination,
                )
            })?;
        }
    }

    sync_parent(
        parent,
        PublicationStage::ParentSyncAfterCleanup,
        PublicationMode::NoClobber,
        candidate,
        destination,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecoveryCandidateState {
    Missing,
    OwnedLink,
}

fn recovery_candidate_state(
    candidate: &Path,
    destination: &Path,
) -> io::Result<RecoveryCandidateState> {
    let destination_metadata = match fs::symlink_metadata(destination) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Err(io::Error::other(
                "recovery destination disappeared after publication",
            ));
        }
        Err(source) => return Err(source),
    };
    if destination_metadata.file_type().is_symlink() || !destination_metadata.is_file() {
        return Err(io::Error::other(
            "recovery destination is not the revalidated regular file",
        ));
    }
    let candidate_metadata = match fs::symlink_metadata(candidate) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Ok(RecoveryCandidateState::Missing);
        }
        Err(source) => return Err(source),
    };
    if candidate_metadata.file_type().is_symlink()
        || !candidate_metadata.is_file()
        || identity_from_metadata(&candidate_metadata)
            != identity_from_metadata(&destination_metadata)
    {
        return Err(io::Error::other(
            "recovery candidate is not the identity-proven destination link",
        ));
    }
    Ok(RecoveryCandidateState::OwnedLink)
}

fn sibling_parent<'a>(candidate: &'a Path, destination: &Path) -> io::Result<&'a Path> {
    let candidate_parent = candidate.parent();
    if !candidate.is_absolute()
        || !destination.is_absolute()
        || candidate == destination
        || candidate_parent.is_none()
        || candidate_parent != destination.parent()
        || candidate.file_name().is_none()
        || destination.file_name().is_none()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "publication paths are not distinct absolute siblings",
        ));
    }
    Ok(candidate_parent.expect("validated candidate parent"))
}

fn sync_parent(
    parent: &Path,
    stage: PublicationStage,
    mode: PublicationMode,
    candidate: &Path,
    destination: &Path,
) -> Result<()> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| native_error(source, stage, mode, candidate, destination))
}

fn native_error(
    source: io::Error,
    stage: PublicationStage,
    mode: PublicationMode,
    candidate: &Path,
    destination: &Path,
) -> Error {
    let native_code = source.raw_os_error().map(|code| format!("errno:{code}"));
    let (category, recoverability) = classify_io_error(&source, stage, mode);
    platform_error(
        source,
        stage,
        category,
        recoverability,
        stage_message(stage, mode),
        native_code,
        candidate,
        destination,
    )
}

fn classify_io_error(
    source: &io::Error,
    stage: PublicationStage,
    mode: PublicationMode,
) -> (ErrorCategory, Recoverability) {
    if let Some(code) = source.raw_os_error() {
        if code == libc::EEXIST {
            return (ErrorCategory::Conflict, Recoverability::UserCorrectable);
        }
        if code == libc::ENOENT {
            return (ErrorCategory::NotFound, Recoverability::UserCorrectable);
        }
        if code == libc::EACCES || code == libc::EROFS {
            return (
                ErrorCategory::PermissionDenied,
                Recoverability::UserCorrectable,
            );
        }
        if code == libc::EPERM {
            return if stage == PublicationStage::NativePublish && mode == PublicationMode::NoClobber
            {
                (ErrorCategory::Unsupported, Recoverability::UserCorrectable)
            } else {
                (
                    ErrorCategory::PermissionDenied,
                    Recoverability::UserCorrectable,
                )
            };
        }
        if code == libc::EXDEV || code == libc::ENOTSUP || code == libc::EOPNOTSUPP {
            return (ErrorCategory::Unsupported, Recoverability::UserCorrectable);
        }
        if code == libc::ENOSPC
            || code == libc::EDQUOT
            || code == libc::ENOMEM
            || code == libc::EMLINK
        {
            return (
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
            );
        }
        if code == libc::ENAMETOOLONG {
            return (ErrorCategory::InvalidInput, Recoverability::UserCorrectable);
        }
        if code == libc::EINVAL {
            return if matches!(
                stage,
                PublicationStage::ParentSyncAfterPublish | PublicationStage::ParentSyncAfterCleanup
            ) {
                (ErrorCategory::Unsupported, Recoverability::UserCorrectable)
            } else {
                (ErrorCategory::InvalidInput, Recoverability::UserCorrectable)
            };
        }
    }

    match source.kind() {
        io::ErrorKind::AlreadyExists => (ErrorCategory::Conflict, Recoverability::UserCorrectable),
        io::ErrorKind::NotFound => (ErrorCategory::NotFound, Recoverability::UserCorrectable),
        io::ErrorKind::PermissionDenied => (
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
        ),
        io::ErrorKind::OutOfMemory => (
            ErrorCategory::ResourceExhausted,
            Recoverability::UserCorrectable,
        ),
        io::ErrorKind::InvalidInput => {
            (ErrorCategory::InvalidInput, Recoverability::UserCorrectable)
        }
        io::ErrorKind::Unsupported => (ErrorCategory::Unsupported, Recoverability::UserCorrectable),
        _ => (ErrorCategory::Unavailable, Recoverability::Retryable),
    }
}

const fn stage_message(stage: PublicationStage, mode: PublicationMode) -> &'static str {
    match stage {
        PublicationStage::NativePublish => match mode {
            PublicationMode::ReplaceExisting => "atomic project replacement failed",
            PublicationMode::NoClobber => "atomic no-clobber project publication failed",
        },
        PublicationStage::ParentSyncAfterPublish => {
            "project destination was published but its directory could not be synchronized"
        }
        PublicationStage::CandidateUnlink => {
            "project destination was published but the candidate name could not be removed"
        }
        PublicationStage::ParentSyncAfterCleanup => {
            "project destination was published but candidate cleanup could not be synchronized"
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "superi-save-unix-{label}-{}-{}",
                std::process::id(),
                NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }

        fn file(&self, name: &str, contents: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            let mut file = File::create(&path).unwrap();
            file.write_all(contents).unwrap();
            file.sync_all().unwrap();
            path
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn replace_existing_publishes_candidate_and_removes_its_old_name() {
        let root = TempRoot::new("replace");
        let candidate = root.file("candidate", b"new");
        let destination = root.file("destination", b"old");

        publish(&candidate, &destination, PublicationMode::ReplaceExisting).unwrap();

        assert_eq!(fs::read(destination).unwrap(), b"new");
        assert!(!candidate.exists());
    }

    #[test]
    fn no_clobber_publishes_one_name_and_removes_candidate_name() {
        let root = TempRoot::new("no-clobber");
        let candidate = root.file("candidate", b"new");
        let destination = root.0.join("destination");

        publish(&candidate, &destination, PublicationMode::NoClobber).unwrap();

        assert_eq!(fs::read(destination).unwrap(), b"new");
        assert!(!candidate.exists());
    }

    #[test]
    fn no_clobber_publish_preserves_a_candidate_replaced_after_first_sync() {
        let root = TempRoot::new("publish-replaced-candidate");
        let candidate = root.file("candidate", b"published");
        let destination = root.0.join("destination");
        let replacement = root.file("replacement", b"unrelated replacement");
        let sentinel = root.file("sentinel", b"preserve");

        let error = publish_with_post_sync_hook(
            &candidate,
            &destination,
            PublicationMode::NoClobber,
            || {
                fs::remove_file(&candidate).unwrap();
                fs::rename(&replacement, &candidate).unwrap();
            },
        )
        .unwrap_err();

        assert_eq!(error.category(), ErrorCategory::Unavailable);
        assert_eq!(error.recoverability(), Recoverability::Retryable);
        assert_eq!(fs::read(candidate).unwrap(), b"unrelated replacement");
        assert_eq!(fs::read(destination).unwrap(), b"published");
        assert_eq!(fs::read(sentinel).unwrap(), b"preserve");
    }

    #[test]
    fn no_clobber_publish_finishes_when_candidate_disappears_after_first_sync() {
        let root = TempRoot::new("publish-missing-candidate");
        let candidate = root.file("candidate", b"published");
        let destination = root.0.join("destination");
        let sentinel = root.file("sentinel", b"preserve");

        publish_with_post_sync_hook(&candidate, &destination, PublicationMode::NoClobber, || {
            fs::remove_file(&candidate).unwrap()
        })
        .unwrap();

        assert!(!candidate.exists());
        assert_eq!(fs::read(destination).unwrap(), b"published");
        assert_eq!(fs::read(sentinel).unwrap(), b"preserve");
    }

    #[test]
    fn no_clobber_race_preserves_both_existing_names() {
        let root = TempRoot::new("race");
        let candidate = root.file("candidate", b"new");
        let destination = root.file("destination", b"winner");

        let error = publish(&candidate, &destination, PublicationMode::NoClobber).unwrap_err();

        assert_eq!(error.category(), ErrorCategory::Conflict);
        assert_eq!(fs::read(candidate).unwrap(), b"new");
        assert_eq!(fs::read(destination).unwrap(), b"winner");
    }

    #[test]
    fn visible_no_clobber_recovery_removes_only_the_owned_candidate_link() {
        let root = TempRoot::new("recover-link");
        let candidate = root.file("candidate", b"published");
        let destination = root.0.join("destination");
        let unrelated = root.file("unrelated", b"preserve");
        fs::hard_link(&candidate, &destination).unwrap();

        recover_visible_no_clobber(&candidate, &destination).unwrap();

        assert!(!candidate.exists());
        assert_eq!(fs::read(destination).unwrap(), b"published");
        assert_eq!(fs::read(unrelated).unwrap(), b"preserve");
    }

    #[test]
    fn file_identity_matches_handles_and_hard_links_but_not_other_files() {
        let root = TempRoot::new("identity");
        let original = root.file("original", b"original");
        let hard_link = root.0.join("hard-link");
        let unrelated = root.file("unrelated", b"unrelated");
        fs::hard_link(&original, &hard_link).unwrap();
        let file = File::open(&original).unwrap();

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
    fn visible_no_clobber_recovery_accepts_an_already_moved_candidate() {
        let root = TempRoot::new("recover-missing");
        let candidate = root.0.join("candidate");
        let destination = root.file("destination", b"published");
        let unrelated = root.file("unrelated", b"preserve");

        recover_visible_no_clobber(&candidate, &destination).unwrap();

        assert!(!candidate.exists());
        assert_eq!(fs::read(destination).unwrap(), b"published");
        assert_eq!(fs::read(unrelated).unwrap(), b"preserve");
    }

    #[test]
    fn visible_no_clobber_recovery_preserves_mismatched_artifacts() {
        let root = TempRoot::new("recover-mismatch");
        let candidate = root.file("candidate", b"candidate");
        let destination = root.file("destination", b"destination");
        let unrelated = root.file("unrelated", b"preserve");

        let error = recover_visible_no_clobber(&candidate, &destination).unwrap_err();

        assert_eq!(error.category(), ErrorCategory::Unavailable);
        assert_eq!(error.recoverability(), Recoverability::Retryable);
        assert_eq!(fs::read(candidate).unwrap(), b"candidate");
        assert_eq!(fs::read(destination).unwrap(), b"destination");
        assert_eq!(fs::read(unrelated).unwrap(), b"preserve");
    }

    #[test]
    fn postvisibility_failure_names_the_exact_native_stage() {
        let root = TempRoot::new("postvisibility-context");
        let candidate = root.0.join("candidate");
        let destination = root.0.join("destination");
        let error = native_error(
            io::Error::other("injected directory sync failure"),
            PublicationStage::ParentSyncAfterPublish,
            PublicationMode::NoClobber,
            &candidate,
            &destination,
        );

        let context = error
            .contexts()
            .iter()
            .find(|context| context.operation() == "publish_project")
            .expect("platform publication context");
        assert_eq!(context.field("phase"), Some("parent_sync_after_publish"));
        assert_eq!(error.category(), ErrorCategory::Unavailable);
        assert_eq!(error.recoverability(), Recoverability::Retryable);
    }

    #[test]
    fn defensive_boundary_rejects_non_sibling_paths() {
        let error = publish(
            Path::new("relative-candidate"),
            Path::new("relative-destination"),
            PublicationMode::NoClobber,
        )
        .unwrap_err();

        assert_eq!(error.category(), ErrorCategory::InvalidInput);
    }

    #[test]
    fn raw_errno_classification_preserves_atomicity_categories() {
        let classify = |code| {
            classify_io_error(
                &io::Error::from_raw_os_error(code),
                PublicationStage::NativePublish,
                PublicationMode::NoClobber,
            )
            .0
        };

        assert_eq!(classify(libc::EEXIST), ErrorCategory::Conflict);
        assert_eq!(classify(libc::EXDEV), ErrorCategory::Unsupported);
        assert_eq!(classify(libc::ENOSPC), ErrorCategory::ResourceExhausted);
        assert_eq!(classify(libc::EACCES), ErrorCategory::PermissionDenied);
    }
}
