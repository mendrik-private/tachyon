use std::{
    ffi::OsStr,
    fmt::Write as _,
    fs::{self, File, OpenOptions},
    io::{self, Read as _, Write as _},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use document_core::{NodeId, Revision};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

static WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static RECOVERY_GATE: Mutex<()> = Mutex::new(());

fn state_root() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .unwrap_or_else(std::env::temp_dir)
}

#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("file changed outside Tachyon: {0}")]
    ExternalChange(PathBuf),
    #[error("persistence I/O failed for {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("recovery journal is malformed: {0}")]
    Journal(String),
    #[error(
        "write to {path} completed, but filesystem durability could not be confirmed: {source}"
    )]
    DurabilityUncertain { path: PathBuf, source: io::Error },
}

#[derive(Debug, thiserror::Error)]
pub enum RecoverableSaveError {
    #[error("{save}; changes were written to recovery storage")]
    Recovered { save: PersistenceError },
    #[error("{save}; recovery storage also failed: {recovery}")]
    Unrecovered {
        save: PersistenceError,
        recovery: PersistenceError,
    },
}

/// The file state a document was loaded from or last written as.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceIdentity {
    pub path: PathBuf,
    pub length: u64,
    pub modified: Option<SystemTime>,
    pub content_hash: [u8; 32],
    #[cfg(unix)]
    pub device: u64,
    #[cfg(unix)]
    pub inode: u64,
}

impl SourceIdentity {
    fn new(path: &Path, metadata: &fs::Metadata, bytes: &[u8]) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt as _;
        Self {
            path: path.to_path_buf(),
            length: metadata.len(),
            modified: metadata.modified().ok(),
            content_hash: Sha256::digest(bytes).into(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        }
    }

    /// The identity `path` has once `file`, which holds exactly `bytes`, is
    /// published there. Publishing by rename or hard link keeps the inode,
    /// length, and modification time, so no read of `path` is needed.
    fn written(path: &Path, file: &File, bytes: &[u8]) -> Result<Self, PersistenceError> {
        let metadata = file
            .metadata()
            .map_err(|error| PersistenceError::io(path, error))?;
        Ok(Self::new(path, &metadata, bytes))
    }
}

/// A save of `bytes` over the file last observed as `expected_identity`.
#[derive(Clone, Debug)]
pub struct SaveSnapshot {
    pub revision: Revision,
    pub bytes: Arc<[u8]>,
    pub expected_identity: SourceIdentity,
}

#[derive(Debug)]
pub struct SaveOutcome {
    pub identity: SourceIdentity,
    pub cleanup_warning: Option<PersistenceError>,
}

#[derive(Debug)]
pub(crate) struct WriteOutcome {
    pub identity: SourceIdentity,
    pub durability_warning: Option<PersistenceError>,
}

impl PersistenceError {
    fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

/// Permissions of a newly written file.
#[derive(Clone, Debug)]
pub(crate) enum FileMode {
    /// Copies these permissions, normally those of the file being replaced.
    Permissions(fs::Permissions),
    /// Readable and writable by the owner only, for files holding document text.
    Private,
    /// The process default.
    Default,
}

impl FileMode {
    fn inherited_from(target: &Path) -> Self {
        fs::metadata(target).map_or(Self::Default, |metadata| {
            Self::Permissions(metadata.permissions())
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WriteStage {
    CreateTemporary,
    Write,
    SyncTemporary,
    Replace,
    SyncDirectory,
}

type StageHook<'a> = &'a mut dyn FnMut(WriteStage, &Path) -> io::Result<()>;

fn parent_directory(path: &Path) -> &Path {
    path.parent().unwrap_or_else(|| Path::new("."))
}

/// Exclusively creates `path`, then writes and syncs `bytes`. A file created
/// here is removed again if a later step fails.
fn create_synced(
    path: &Path,
    bytes: &[u8],
    mode: &FileMode,
    before: StageHook<'_>,
) -> Result<File, PersistenceError> {
    before(WriteStage::CreateTemporary, path).map_err(|error| PersistenceError::io(path, error))?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if matches!(mode, FileMode::Private) {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| PersistenceError::io(path, error))?;
    let written = (|| {
        if let FileMode::Permissions(permissions) = mode {
            file.set_permissions(permissions.clone())?;
        }
        before(WriteStage::Write, path)?;
        file.write_all(bytes)?;
        before(WriteStage::SyncTemporary, path)?;
        file.sync_all()
    })();
    match written {
        Ok(()) => Ok(file),
        Err(error) => {
            drop(file);
            let _ = fs::remove_file(path);
            Err(PersistenceError::io(path, error))
        }
    }
}

/// A fully written and synced file at a hidden, process-unique sibling of
/// its target. The staged name is removed on drop unless it was published.
struct StagedFile {
    path: PathBuf,
    file: File,
    staged: bool,
}

impl StagedFile {
    fn write(
        target: &Path,
        purpose: &str,
        bytes: &[u8],
        mode: &FileMode,
        before: StageHook<'_>,
    ) -> Result<Self, PersistenceError> {
        let name = target
            .file_name()
            .unwrap_or_else(|| OsStr::new("document.md"));
        let path = parent_directory(target).join(format!(
            ".{}.tachyon-{purpose}-{}-{}",
            name.to_string_lossy(),
            std::process::id(),
            WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let file = create_synced(&path, bytes, mode, before)?;
        Ok(Self {
            path,
            file,
            staged: true,
        })
    }

    /// Atomically replaces `target` with the staged file.
    fn replace(&mut self, target: &Path) -> Result<(), PersistenceError> {
        fs::rename(&self.path, target).map_err(|error| PersistenceError::io(target, error))?;
        self.staged = false;
        Ok(())
    }

    /// Publishes the staged file at `target` through `link`, which must never
    /// replace an existing file, then drops the staged name. The returned
    /// warning reports a staged name that could not be removed.
    fn link_new(
        &mut self,
        target: &Path,
        link: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<Option<PersistenceError>> {
        link(&self.path, target)?;
        Ok(self.discard())
    }

    fn discard(&mut self) -> Option<PersistenceError> {
        self.staged = false;
        fs::remove_file(&self.path)
            .err()
            .map(|error| PersistenceError::io(&self.path, error))
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        if self.staged {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn sync_directory(directory: &Path) -> io::Result<()> {
    File::open(directory).and_then(|directory| directory.sync_all())
}

fn durability_warning(target: &Path, synced: io::Result<()>) -> Option<PersistenceError> {
    synced
        .err()
        .map(|source| PersistenceError::DurabilityUncertain {
            path: target.to_path_buf(),
            source,
        })
}

/// Atomically replaces `target`, creating its directory when needed. An error
/// means `target` was left untouched; a returned warning means it was
/// replaced but the directory entry may not be durable yet.
pub(crate) fn replace_atomically(
    target: &Path,
    purpose: &str,
    bytes: &[u8],
    mode: &FileMode,
) -> Result<Option<PersistenceError>, PersistenceError> {
    let parent = parent_directory(target);
    fs::create_dir_all(parent).map_err(|error| PersistenceError::io(parent, error))?;
    let mut staged = StagedFile::write(target, purpose, bytes, mode, &mut |_, _| Ok(()))?;
    staged.replace(target)?;
    Ok(durability_warning(target, sync_directory(parent)))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalState {
    Unchanged,
    Modified(SourceIdentity),
    Deleted,
}

pub fn source_identity(path: &Path) -> Result<SourceIdentity, PersistenceError> {
    read_source_bytes_with_identity(path).map(|(_, identity)| identity)
}

/// The identity of the file at `path`, or `None` when nothing exists there.
pub(crate) fn existing_identity(path: &Path) -> Result<Option<SourceIdentity>, PersistenceError> {
    match source_identity(path) {
        Ok(identity) => Ok(Some(identity)),
        Err(PersistenceError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

pub fn read_source_with_identity(
    path: &Path,
) -> Result<(String, SourceIdentity), PersistenceError> {
    let (bytes, identity) = read_source_bytes_with_identity(path)?;
    let source = String::from_utf8(bytes).map_err(|error| {
        PersistenceError::io(path, io::Error::new(io::ErrorKind::InvalidData, error))
    })?;
    Ok((source, identity))
}

fn read_source_bytes_with_identity(
    path: &Path,
) -> Result<(Vec<u8>, SourceIdentity), PersistenceError> {
    read_source_bytes_with_identity_using(path, |_| Ok(()))
}

fn read_source_bytes_with_identity_using(
    path: &Path,
    after_read: impl FnOnce(&Path) -> io::Result<()>,
) -> Result<(Vec<u8>, SourceIdentity), PersistenceError> {
    let mut file = File::open(path).map_err(|error| PersistenceError::io(path, error))?;
    let before = file
        .metadata()
        .map_err(|error| PersistenceError::io(path, error))?;
    let mut bytes = Vec::with_capacity(usize::try_from(before.len()).unwrap_or_default());
    file.read_to_end(&mut bytes)
        .map_err(|error| PersistenceError::io(path, error))?;
    after_read(path).map_err(|error| PersistenceError::io(path, error))?;
    let after = file
        .metadata()
        .map_err(|error| PersistenceError::io(path, error))?;
    let path_after = fs::metadata(path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            PersistenceError::ExternalChange(path.to_path_buf())
        } else {
            PersistenceError::io(path, error)
        }
    })?;
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt as _;
    let stable = before.len() == after.len()
        && before.modified().ok() == after.modified().ok()
        && bytes.len() as u64 == after.len()
        && path_after.len() == after.len()
        && path_after.modified().ok() == after.modified().ok()
        && {
            #[cfg(unix)]
            {
                before.dev() == after.dev()
                    && before.ino() == after.ino()
                    && path_after.dev() == after.dev()
                    && path_after.ino() == after.ino()
            }
            #[cfg(not(unix))]
            {
                true
            }
        };
    if !stable {
        return Err(PersistenceError::ExternalChange(path.to_path_buf()));
    }
    let identity = SourceIdentity::new(path, &after, &bytes);
    Ok((bytes, identity))
}

pub fn detect_external_state(
    path: &Path,
    expected: &SourceIdentity,
) -> Result<ExternalState, PersistenceError> {
    match source_identity(path) {
        Ok(current) if current == *expected => Ok(ExternalState::Unchanged),
        Ok(current) => Ok(ExternalState::Modified(current)),
        Err(PersistenceError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
            Ok(ExternalState::Deleted)
        }
        Err(error) => Err(error),
    }
}

fn require_expected_identity(
    path: &Path,
    expected: &SourceIdentity,
) -> Result<(), PersistenceError> {
    match detect_external_state(path, expected)? {
        ExternalState::Unchanged => Ok(()),
        ExternalState::Modified(_) | ExternalState::Deleted => {
            Err(PersistenceError::ExternalChange(path.to_path_buf()))
        }
    }
}

/// Writes `bytes` to `path`, whose state was just observed by
/// [`existing_identity`]. An observed file is atomically replaced if it is
/// still unchanged right before the replacement; a missing file is created
/// without replacing anything that appeared meanwhile.
pub(crate) fn write_target(
    path: &Path,
    observed: Option<&SourceIdentity>,
    bytes: &[u8],
) -> Result<WriteOutcome, PersistenceError> {
    match observed {
        Some(expected) => {
            debug_assert!(expected.path.as_path() == path);
            replace_verified(expected, bytes, &mut |_, _| Ok(()))
        }
        None => atomic_write_new(path, bytes),
    }
}

fn atomic_save(snapshot: SaveSnapshot) -> Result<WriteOutcome, PersistenceError> {
    atomic_save_with_hook(snapshot, |_, _| Ok(()))
}

fn atomic_save_with_hook(
    snapshot: SaveSnapshot,
    mut before: impl FnMut(WriteStage, &Path) -> io::Result<()>,
) -> Result<WriteOutcome, PersistenceError> {
    let expected = &snapshot.expected_identity;
    require_expected_identity(&expected.path, expected)?;
    replace_verified(expected, &snapshot.bytes, &mut before)
}

/// Replaces the file last observed as `expected`, keeping its permissions.
/// The identity is checked again immediately before the rename so an
/// external edit made while the replacement was staged is never overwritten.
fn replace_verified(
    expected: &SourceIdentity,
    bytes: &[u8],
    before: StageHook<'_>,
) -> Result<WriteOutcome, PersistenceError> {
    let path = expected.path.as_path();
    let parent = parent_directory(path);
    let mut staged = StagedFile::write(
        path,
        "save",
        bytes,
        &FileMode::inherited_from(path),
        &mut *before,
    )?;
    let identity = SourceIdentity::written(path, &staged.file, bytes)?;
    before(WriteStage::Replace, path).map_err(|error| PersistenceError::io(path, error))?;
    require_expected_identity(path, expected)?;
    staged.replace(path)?;
    let synced = before(WriteStage::SyncDirectory, parent).and_then(|()| sync_directory(parent));
    Ok(WriteOutcome {
        identity,
        durability_warning: durability_warning(path, synced),
    })
}

pub fn save_with_recovery(
    snapshot: SaveSnapshot,
    journal: &RecoveryJournal,
) -> Result<SaveOutcome, RecoverableSaveError> {
    save_with_recovery_using(snapshot, journal, atomic_save)
}

fn save_with_recovery_using(
    snapshot: SaveSnapshot,
    journal: &RecoveryJournal,
    save: impl FnOnce(SaveSnapshot) -> Result<WriteOutcome, PersistenceError>,
) -> Result<SaveOutcome, RecoverableSaveError> {
    let source_path = snapshot.expected_identity.path.clone();
    let revision = snapshot.revision;
    let entry = RecoveryEntry::new(
        source_path.clone(),
        revision,
        String::from_utf8_lossy(&snapshot.bytes).into_owned(),
        Some(snapshot.expected_identity.clone()),
    );
    let recovery_error = journal.write(&entry).err();

    match save(snapshot) {
        Ok(outcome) => {
            // Edits made while the save ran may already have journaled a
            // newer revision; only the saved revision is safe to drop.
            let cleanup_warning = match outcome.durability_warning {
                Some(warning) => Some(warning),
                None => journal.clear_revision(&source_path, revision).err(),
            };
            Ok(SaveOutcome {
                identity: outcome.identity,
                cleanup_warning,
            })
        }
        Err(save) => match recovery_error {
            None => Err(RecoverableSaveError::Recovered { save }),
            Some(recovery) => Err(RecoverableSaveError::Unrecovered { save, recovery }),
        },
    }
}

/// Atomically creates a new file without replacing an existing path. A hard
/// link publishes the fully synced temporary inode, which gives copy saves the
/// same-directory atomicity of normal saves while retaining create-new safety.
/// Filesystems without hard links (vfat, exFAT, many FUSE and SMB mounts) fall
/// back to an exclusive create of the target, which still never replaces an
/// existing file but is not atomic against a crash mid-write.
fn atomic_write_new(path: &Path, bytes: &[u8]) -> Result<WriteOutcome, PersistenceError> {
    atomic_write_new_using(path, bytes, |temporary, target| {
        fs::hard_link(temporary, target)
    })
}

fn hard_link_unsupported(error: &io::Error) -> bool {
    // EPERM maps to PermissionDenied and EOPNOTSUPP/ENOTSUP to Unsupported.
    matches!(
        error.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::Unsupported
    )
}

fn atomic_write_new_using(
    path: &Path,
    bytes: &[u8],
    link: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> Result<WriteOutcome, PersistenceError> {
    let parent = parent_directory(path);
    fs::create_dir_all(parent).map_err(|error| PersistenceError::io(parent, error))?;
    let mut staged =
        StagedFile::write(path, "copy", bytes, &FileMode::Default, &mut |_, _| Ok(()))?;
    let linked_identity = SourceIdentity::written(path, &staged.file, bytes)?;
    let (identity, cleanup_warning) = match staged.link_new(path, link) {
        Ok(cleanup_warning) => (linked_identity, cleanup_warning),
        Err(error) if hard_link_unsupported(&error) => {
            let cleanup_warning = staged.discard();
            let file = create_synced(path, bytes, &FileMode::Default, &mut |_, _| Ok(()))?;
            (
                SourceIdentity::written(path, &file, bytes)?,
                cleanup_warning,
            )
        }
        Err(error) => return Err(PersistenceError::io(path, error)),
    };
    // Uncertain durability of the new file outranks a leftover staged name,
    // which is then only logged.
    let durability_warning = match (
        durability_warning(path, sync_directory(parent)),
        cleanup_warning,
    ) {
        (Some(durability), Some(cleanup)) => {
            eprintln!("copy save could not remove its temporary file: {cleanup}");
            Some(durability)
        }
        (durability, cleanup) => durability.or(cleanup),
    };
    Ok(WriteOutcome {
        identity,
        durability_warning,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryEntry {
    pub source_path: PathBuf,
    pub revision: u64,
    pub markdown: String,
    #[serde(default)]
    pub base_identity: Option<SourceIdentity>,
    pub written_at_unix_ms: u128,
}

impl RecoveryEntry {
    #[must_use]
    pub fn new(
        source_path: PathBuf,
        revision: Revision,
        markdown: String,
        base_identity: Option<SourceIdentity>,
    ) -> Self {
        Self {
            source_path,
            revision: revision.0,
            markdown,
            base_identity,
            written_at_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        }
    }
}

/// A recovery record the user has not yet restored or dismissed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnresolvedRecovery {
    pub entry: RecoveryEntry,
    /// Whether this load copied the record into the pending sidecar.
    pub promoted: bool,
}

#[derive(Clone, Debug)]
pub struct RecoveryJournal {
    directory: PathBuf,
}

impl RecoveryJournal {
    #[must_use]
    pub fn for_current_user() -> Self {
        Self {
            directory: state_root().join("tachyon/recovery"),
        }
    }

    #[cfg(test)]
    fn in_directory(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn write(&self, entry: &RecoveryEntry) -> Result<(), PersistenceError> {
        let _gate = RECOVERY_GATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.write_unlocked(&self.path_for(&entry.source_path), entry)
    }

    fn write_unlocked(&self, target: &Path, entry: &RecoveryEntry) -> Result<(), PersistenceError> {
        let bytes = serde_json::to_vec(entry)
            .map_err(|error| PersistenceError::Journal(error.to_string()))?;
        // The record is in place once replaced; callers have no channel for
        // a durability warning, so it is logged.
        if let Some(warning) = replace_atomically(target, "journal", &bytes, &FileMode::Private)? {
            eprintln!("recovery journal: {warning}");
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn load(&self, source_path: &Path) -> Result<Option<RecoveryEntry>, PersistenceError> {
        let _gate = RECOVERY_GATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.load_unlocked(source_path)
    }

    /// Loads the recovery record a newly opened document should offer. The
    /// reopened document journals its own edits under the same key, so a
    /// record found there is first preserved in a pending sidecar that only
    /// [`Self::restore_pending`] or [`Self::dismiss_pending`] removes. An
    /// existing sidecar wins over the main record, which may be a live draft
    /// written by another view of the same document.
    pub fn load_unresolved(
        &self,
        source_path: &Path,
    ) -> Result<Option<UnresolvedRecovery>, PersistenceError> {
        let _gate = RECOVERY_GATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let pending = self.pending_path_for(source_path);
        if let Some(entry) = Self::read_record(&pending)? {
            return Ok(Some(UnresolvedRecovery {
                entry,
                promoted: false,
            }));
        }
        let Some(entry) = self.load_unlocked(source_path)? else {
            return Ok(None);
        };
        self.write_unlocked(&pending, &entry)?;
        Ok(Some(UnresolvedRecovery {
            entry,
            promoted: true,
        }))
    }

    /// Removes the pending sidecar if it still holds `entry`, leaving the
    /// main record untouched.
    pub fn release_pending(&self, entry: &RecoveryEntry) -> Result<(), PersistenceError> {
        let _gate = RECOVERY_GATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.release_pending_unlocked(entry)
    }

    /// Resolves a restored record: the restored draft is journaled under the
    /// document's current key before the pending sidecar is dropped, so a
    /// crash after restoring still leaves the draft recoverable.
    pub fn restore_pending(
        &self,
        entry: &RecoveryEntry,
        restored: &RecoveryEntry,
    ) -> Result<(), PersistenceError> {
        let _gate = RECOVERY_GATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.write_unlocked(&self.path_for(&restored.source_path), restored)?;
        self.release_pending_unlocked(entry)
    }

    /// Resolves a dismissed record by removing it from the sidecar and from
    /// the main key, unless newer drafts have already replaced it there.
    pub fn dismiss_pending(&self, entry: &RecoveryEntry) -> Result<(), PersistenceError> {
        let _gate = RECOVERY_GATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.release_pending_unlocked(entry)?;
        if self
            .load_unlocked(&entry.source_path)?
            .is_some_and(|current| current == *entry)
        {
            self.clear_unlocked(&entry.source_path)?;
        }
        Ok(())
    }

    fn release_pending_unlocked(&self, entry: &RecoveryEntry) -> Result<(), PersistenceError> {
        let pending = self.pending_path_for(&entry.source_path);
        if Self::read_record(&pending)?.is_some_and(|current| current == *entry) {
            self.remove_record(&pending)?;
        }
        Ok(())
    }

    fn load_unlocked(&self, source_path: &Path) -> Result<Option<RecoveryEntry>, PersistenceError> {
        Self::read_record(&self.path_for(source_path))
    }

    fn read_record(path: &Path) -> Result<Option<RecoveryEntry>, PersistenceError> {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| PersistenceError::Journal(error.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(PersistenceError::io(path, error)),
        }
    }

    pub fn clear(&self, source_path: &Path) -> Result<(), PersistenceError> {
        let _gate = RECOVERY_GATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.clear_unlocked(source_path)
    }

    pub fn clear_revision(
        &self,
        source_path: &Path,
        revision: Revision,
    ) -> Result<(), PersistenceError> {
        let _gate = RECOVERY_GATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self
            .load_unlocked(source_path)?
            .is_some_and(|entry| entry.revision == revision.0)
        {
            self.clear_unlocked(source_path)?;
        }
        Ok(())
    }

    fn clear_unlocked(&self, source_path: &Path) -> Result<(), PersistenceError> {
        self.remove_record(&self.path_for(source_path))
    }

    fn remove_record(&self, path: &Path) -> Result<(), PersistenceError> {
        match fs::remove_file(path) {
            Ok(()) => sync_directory(&self.directory)
                .map_err(|error| PersistenceError::io(&self.directory, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(PersistenceError::io(path, error)),
        }
    }

    fn path_for(&self, source_path: &Path) -> PathBuf {
        self.directory
            .join(format!("v1-{}.json", Self::key_for(source_path)))
    }

    fn pending_path_for(&self, source_path: &Path) -> PathBuf {
        self.directory
            .join(format!("v1-{}.pending.json", Self::key_for(source_path)))
    }

    fn key_for(source_path: &Path) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"tachyon-recovery-v1\0");
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;
            hasher.update(source_path.as_os_str().as_bytes());
        }
        #[cfg(not(unix))]
        hasher.update(source_path.to_string_lossy().as_bytes());
        let digest = hasher.finalize();
        let mut key = String::with_capacity(digest.len() * 2);
        for byte in digest {
            let _ = write!(key, "{byte:02x}");
        }
        key
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WorkspaceState {
    pub active_path: Option<PathBuf>,
    pub last_open_directory: Option<PathBuf>,
    #[serde(default)]
    pub draft_recovery_key: Option<PathBuf>,
    #[serde(default)]
    pub navigation_root: Option<PathBuf>,
    #[serde(default)]
    pub navigation_root_explicit: bool,
    pub navigation_width: f32,
    pub justify: bool,
    pub hyphenate: bool,
    pub expanded_folders: Vec<PathBuf>,
    pub selection_start: usize,
    pub selection_end: usize,
    pub selection_reversed: bool,
    pub scroll_y: f32,
    pub scroll_anchor_node: Option<NodeId>,
    pub scroll_anchor_text_hint: String,
    pub scroll_anchor_text_offset: usize,
    pub scroll_anchor_projection_offset: usize,
    pub scroll_anchor_intra_line_offset: f32,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            active_path: None,
            last_open_directory: None,
            draft_recovery_key: None,
            navigation_root: None,
            navigation_root_explicit: false,
            navigation_width: 224.,
            justify: false,
            hyphenate: false,
            expanded_folders: Vec::new(),
            selection_start: 0,
            selection_end: 0,
            selection_reversed: false,
            scroll_y: 0.,
            scroll_anchor_node: None,
            scroll_anchor_text_hint: String::new(),
            scroll_anchor_text_offset: 0,
            scroll_anchor_projection_offset: 0,
            scroll_anchor_intra_line_offset: 0.,
        }
    }
}

#[derive(Clone, Debug)]
pub struct WorkspaceStateStore {
    path: PathBuf,
}

impl WorkspaceStateStore {
    #[must_use]
    pub fn for_current_user() -> Self {
        Self {
            path: state_root().join("tachyon/workspace.json"),
        }
    }

    #[cfg(test)]
    pub(crate) fn at_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<WorkspaceState, PersistenceError> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| PersistenceError::Journal(error.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(WorkspaceState::default()),
            Err(error) => Err(PersistenceError::io(&self.path, error)),
        }
    }

    /// Writes the state privately, since the scroll anchor hint holds
    /// document text.
    pub fn write(&self, state: &WorkspaceState) -> Result<(), PersistenceError> {
        let bytes = serde_json::to_vec(state)
            .map_err(|error| PersistenceError::Journal(error.to_string()))?;
        // The state is in place once replaced; its caller only reports
        // failures, so a durability warning is logged.
        if let Some(warning) =
            replace_atomically(&self.path, "workspace", &bytes, &FileMode::Private)?
        {
            eprintln!("workspace state: {warning}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory(label: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "tachyon-{label}-{}-{}",
            std::process::id(),
            WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).expect("temporary test directory");
        directory
    }

    #[test]
    fn atomic_save_preserves_permissions_and_rejects_external_changes() {
        let directory = temporary_directory("atomic-save");
        let path = directory.join("document.md");
        fs::write(&path, "old").expect("fixture");
        let identity = source_identity(&path).expect("identity");
        let permissions = fs::metadata(&path).expect("metadata").permissions();
        atomic_save(SaveSnapshot {
            revision: Revision(1),
            bytes: b"new".as_slice().into(),
            expected_identity: identity.clone(),
        })
        .expect("save");
        assert_eq!(fs::read_to_string(&path).expect("saved"), "new");
        assert_eq!(
            fs::metadata(&path).expect("metadata").permissions(),
            permissions
        );

        let error = atomic_save(SaveSnapshot {
            revision: Revision(2),
            bytes: b"overwrite".as_slice().into(),
            expected_identity: identity,
        })
        .expect_err("stale identity must conflict");
        assert!(matches!(error, PersistenceError::ExternalChange(_)));
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[test]
    fn external_rename_delete_and_replacement_are_detected() {
        let directory = temporary_directory("external-lifecycle");
        let path = directory.join("document.md");
        let renamed = directory.join("renamed.md");
        fs::write(&path, "original").expect("fixture");
        let identity = source_identity(&path).expect("identity");

        fs::rename(&path, &renamed).expect("external rename");
        assert_eq!(
            detect_external_state(&path, &identity).expect("deleted state"),
            ExternalState::Deleted
        );

        fs::write(&path, "replacement with a distinct identity").expect("replacement");
        assert!(matches!(
            detect_external_state(&path, &identity).expect("modified state"),
            ExternalState::Modified(_)
        ));
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[cfg(unix)]
    #[test]
    fn source_read_rejects_path_replacement_after_bytes_are_captured() {
        let directory = temporary_directory("read-replacement");
        let path = directory.join("document.md");
        let displaced = directory.join("displaced.md");
        fs::write(&path, "first").expect("fixture");

        let error = read_source_bytes_with_identity_using(&path, |target| {
            fs::rename(target, &displaced)?;
            fs::write(target, "other")
        })
        .expect_err("replacement path must conflict with the captured bytes");

        assert!(matches!(error, PersistenceError::ExternalChange(_)));
        assert_eq!(fs::read_to_string(&path).expect("replacement"), "other");
        assert_eq!(
            fs::read_to_string(&displaced).expect("captured file"),
            "first"
        );
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[test]
    fn disk_full_during_write_keeps_original_and_recovery() {
        let directory = temporary_directory("disk-full");
        let recovery_directory = temporary_directory("disk-full-recovery");
        let journal = RecoveryJournal::in_directory(recovery_directory.clone());
        let path = directory.join("document.md");
        fs::write(&path, "old").expect("fixture");
        let snapshot = SaveSnapshot {
            revision: Revision(4),
            bytes: b"unsaved draft".as_slice().into(),
            expected_identity: source_identity(&path).expect("identity"),
        };

        let error = save_with_recovery_using(snapshot, &journal, |snapshot| {
            atomic_save_with_hook(snapshot, |stage, _| {
                if stage == WriteStage::Write {
                    Err(io::Error::new(
                        io::ErrorKind::StorageFull,
                        "injected full filesystem",
                    ))
                } else {
                    Ok(())
                }
            })
        })
        .expect_err("injected disk-full failure");

        assert!(matches!(error, RecoverableSaveError::Recovered { .. }));
        assert_eq!(fs::read_to_string(&path).expect("original"), "old");
        assert_eq!(
            journal
                .load(&path)
                .expect("journal load")
                .expect("recovery entry")
                .markdown,
            "unsaved draft"
        );
        assert!(!fs::read_dir(&directory).expect("directory").any(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .contains("tachyon-save")
        }));
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
        fs::remove_dir_all(recovery_directory).expect("cleanup recovery directory");
    }

    #[test]
    fn external_edit_during_save_is_not_replaced() {
        let directory = temporary_directory("save-race");
        let path = directory.join("document.md");
        fs::write(&path, "old").expect("fixture");
        let snapshot = SaveSnapshot {
            revision: Revision(8),
            bytes: b"local draft".as_slice().into(),
            expected_identity: source_identity(&path).expect("identity"),
        };

        let error = atomic_save_with_hook(snapshot, |stage, target| {
            if stage == WriteStage::Replace {
                fs::write(target, "external edit wins")?;
            }
            Ok(())
        })
        .expect_err("late external edit must conflict");

        assert!(matches!(error, PersistenceError::ExternalChange(_)));
        assert_eq!(
            fs::read_to_string(&path).expect("external contents"),
            "external edit wins"
        );
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[cfg(unix)]
    #[test]
    fn permission_failure_keeps_original_and_recovery() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = temporary_directory("permission-failure");
        let recovery_directory = temporary_directory("permission-recovery");
        let journal = RecoveryJournal::in_directory(recovery_directory.clone());
        let path = directory.join("document.md");
        fs::write(&path, "old").expect("fixture");
        let snapshot = SaveSnapshot {
            revision: Revision(9),
            bytes: b"unsaved draft".as_slice().into(),
            expected_identity: source_identity(&path).expect("identity"),
        };
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o500))
            .expect("make directory read-only");

        let result = save_with_recovery(snapshot, &journal);
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .expect("restore directory permissions");

        assert!(matches!(
            result,
            Err(RecoverableSaveError::Recovered { .. })
        ));
        assert_eq!(fs::read_to_string(&path).expect("original"), "old");
        assert_eq!(
            journal
                .load(&path)
                .expect("journal load")
                .expect("recovery entry")
                .markdown,
            "unsaved draft"
        );
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
        fs::remove_dir_all(recovery_directory).expect("cleanup recovery directory");
    }

    #[test]
    fn recovery_is_durable_before_target_replacement() {
        let directory = temporary_directory("crash-recovery");
        let recovery_directory = temporary_directory("crash-recovery-journal");
        let journal = RecoveryJournal::in_directory(recovery_directory.clone());
        let path = directory.join("document.md");
        fs::write(&path, "old").expect("fixture");
        let snapshot = SaveSnapshot {
            revision: Revision(11),
            bytes: b"latest draft".as_slice().into(),
            expected_identity: source_identity(&path).expect("identity"),
        };

        let outcome = save_with_recovery_using(snapshot, &journal, |snapshot| {
            assert_eq!(
                journal
                    .load(&path)
                    .expect("journal load")
                    .expect("entry before replacement")
                    .markdown,
                "latest draft"
            );
            atomic_save(snapshot)
        })
        .expect("save");

        assert!(outcome.cleanup_warning.is_none());
        assert_eq!(fs::read_to_string(&path).expect("saved"), "latest draft");
        assert_eq!(journal.load(&path).expect("journal cleared"), None);
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
        fs::remove_dir_all(recovery_directory).expect("cleanup recovery directory");
    }

    #[test]
    fn directory_sync_failure_reports_committed_write_and_retains_recovery() {
        let directory = temporary_directory("uncertain-durability");
        let recovery_directory = temporary_directory("uncertain-durability-recovery");
        let journal = RecoveryJournal::in_directory(recovery_directory.clone());
        let path = directory.join("document.md");
        fs::write(&path, "old").expect("fixture");
        let snapshot = SaveSnapshot {
            revision: Revision(12),
            bytes: b"committed draft".as_slice().into(),
            expected_identity: source_identity(&path).expect("identity"),
        };

        let outcome = save_with_recovery_using(snapshot, &journal, |snapshot| {
            atomic_save_with_hook(snapshot, |stage, _| {
                if stage == WriteStage::SyncDirectory {
                    Err(io::Error::other("injected directory sync failure"))
                } else {
                    Ok(())
                }
            })
        })
        .expect("replacement is a committed save with a durability warning");

        assert_eq!(
            fs::read_to_string(&path).expect("saved target"),
            "committed draft"
        );
        assert_eq!(
            outcome.identity,
            source_identity(&path).expect("reconciled identity")
        );
        assert!(matches!(
            outcome.cleanup_warning,
            Some(PersistenceError::DurabilityUncertain { path: ref warning_path, .. })
                if warning_path == &path
        ));
        assert_eq!(
            journal
                .load(&path)
                .expect("journal load")
                .expect("recovery retained")
                .markdown,
            "committed draft"
        );
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
        fs::remove_dir_all(recovery_directory).expect("cleanup recovery directory");
    }

    #[test]
    fn failures_before_replacement_keep_original_and_recovery_at_every_stage() {
        for failed_stage in [
            WriteStage::CreateTemporary,
            WriteStage::Write,
            WriteStage::SyncTemporary,
            WriteStage::Replace,
        ] {
            let directory = temporary_directory("pre-replace-failure");
            let recovery_directory = temporary_directory("pre-replace-recovery");
            let journal = RecoveryJournal::in_directory(recovery_directory.clone());
            let path = directory.join("document.md");
            fs::write(&path, "old").expect("fixture");
            let snapshot = SaveSnapshot {
                revision: Revision(13),
                bytes: b"uncommitted draft".as_slice().into(),
                expected_identity: source_identity(&path).expect("identity"),
            };

            let result = save_with_recovery_using(snapshot, &journal, |snapshot| {
                atomic_save_with_hook(snapshot, |stage, _| {
                    if stage == failed_stage {
                        Err(io::Error::other("injected pre-replacement failure"))
                    } else {
                        Ok(())
                    }
                })
            });

            assert!(
                matches!(result, Err(RecoverableSaveError::Recovered { .. })),
                "stage {failed_stage:?} must remain a failed save"
            );
            assert_eq!(fs::read_to_string(&path).expect("original"), "old");
            assert_eq!(
                journal
                    .load(&path)
                    .expect("journal load")
                    .expect("recovery retained")
                    .markdown,
                "uncommitted draft"
            );
            assert!(!fs::read_dir(&directory).expect("directory").any(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .contains("tachyon-save")
            }));
            fs::remove_dir_all(directory).expect("cleanup isolated test directory");
            fs::remove_dir_all(recovery_directory).expect("cleanup recovery directory");
        }
    }

    #[cfg(unix)]
    #[test]
    fn journal_cleanup_failure_keeps_committed_identity_and_recovery() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = temporary_directory("cleanup-failure");
        let recovery_directory = temporary_directory("cleanup-failure-recovery");
        let journal = RecoveryJournal::in_directory(recovery_directory.clone());
        let path = directory.join("document.md");
        fs::write(&path, "old").expect("fixture");
        let snapshot = SaveSnapshot {
            revision: Revision(14),
            bytes: b"durable draft".as_slice().into(),
            expected_identity: source_identity(&path).expect("identity"),
        };

        let outcome = save_with_recovery_using(snapshot, &journal, |snapshot| {
            let outcome = atomic_save(snapshot)?;
            fs::set_permissions(&recovery_directory, fs::Permissions::from_mode(0o500))
                .expect("block recovery cleanup");
            Ok(outcome)
        })
        .expect("save remains committed when cleanup fails");
        fs::set_permissions(&recovery_directory, fs::Permissions::from_mode(0o700))
            .expect("restore recovery permissions");

        assert_eq!(
            fs::read_to_string(&path).expect("saved target"),
            "durable draft"
        );
        assert_eq!(
            outcome.identity,
            source_identity(&path).expect("saved identity")
        );
        assert!(matches!(
            outcome.cleanup_warning,
            Some(PersistenceError::Io { path: ref warning_path, .. })
                if warning_path == &journal.path_for(&path)
        ));
        assert_eq!(
            journal
                .load(&path)
                .expect("journal load")
                .expect("recovery retained")
                .markdown,
            "durable draft"
        );
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
        fs::remove_dir_all(recovery_directory).expect("cleanup recovery directory");
    }

    #[test]
    fn save_clears_only_its_own_journal_revision() {
        let directory = temporary_directory("save-newer-journal");
        let recovery_directory = temporary_directory("save-newer-journal-recovery");
        let journal = RecoveryJournal::in_directory(recovery_directory.clone());
        let path = directory.join("document.md");
        fs::write(&path, "old").expect("fixture");
        let snapshot = SaveSnapshot {
            revision: Revision(20),
            bytes: b"saved draft".as_slice().into(),
            expected_identity: source_identity(&path).expect("identity"),
        };

        // An edit journals revision 21 while revision 20 is being saved.
        save_with_recovery_using(snapshot, &journal, |snapshot| {
            journal.write(&RecoveryEntry::new(
                path.clone(),
                Revision(21),
                "newer draft".into(),
                None,
            ))?;
            atomic_save(snapshot)
        })
        .expect("save");
        let newer = journal
            .load(&path)
            .expect("journal load")
            .expect("newer revision survives the older save");
        assert_eq!(
            (newer.revision, newer.markdown.as_str()),
            (21, "newer draft")
        );

        let snapshot = SaveSnapshot {
            revision: Revision(21),
            bytes: b"newer draft".as_slice().into(),
            expected_identity: source_identity(&path).expect("identity"),
        };
        save_with_recovery(snapshot, &journal).expect("save newer revision");
        assert_eq!(journal.load(&path).expect("journal cleared"), None);
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
        fs::remove_dir_all(recovery_directory).expect("cleanup recovery directory");
    }

    #[test]
    fn unresolved_recovery_survives_new_journal_writes_and_saves() {
        let directory = temporary_directory("journal-pending");
        let journal = RecoveryJournal::in_directory(directory.clone());
        let source = directory.join("source.md");
        let previous = RecoveryEntry::new(source.clone(), Revision(5), "crash draft".into(), None);
        journal.write(&previous).expect("previous session record");

        let loaded = journal
            .load_unresolved(&source)
            .expect("load")
            .expect("record offered");
        assert_eq!(loaded.entry, previous);
        assert!(loaded.promoted);
        assert!(journal.pending_path_for(&source).exists());

        // The reopened document journals, saves, and clears its own drafts.
        let live = RecoveryEntry::new(source.clone(), Revision(5), "live edit".into(), None);
        journal.write(&live).expect("live draft");
        journal
            .clear_revision(&source, Revision(5))
            .expect("save cleanup");
        journal.clear(&source).expect("discard cleanup");

        let reloaded = journal
            .load_unresolved(&source)
            .expect("reload")
            .expect("unresolved record is still offered");
        assert_eq!(reloaded.entry, previous);
        assert!(!reloaded.promoted);

        // A sidecar wins over a newer main record, which is not promoted.
        journal.write(&live).expect("live draft");
        assert_eq!(
            journal
                .load_unresolved(&source)
                .expect("load")
                .expect("record")
                .entry,
            previous
        );
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[test]
    fn dismissing_unresolved_recovery_removes_it_but_keeps_newer_drafts() {
        let directory = temporary_directory("journal-dismiss");
        let journal = RecoveryJournal::in_directory(directory.clone());
        let source = directory.join("source.md");
        let previous = RecoveryEntry::new(source.clone(), Revision(3), "crash draft".into(), None);
        journal.write(&previous).expect("previous session record");
        let offered = journal
            .load_unresolved(&source)
            .expect("load")
            .expect("record offered")
            .entry;

        journal.dismiss_pending(&offered).expect("dismiss");
        assert!(!journal.pending_path_for(&source).exists());
        assert_eq!(journal.load(&source).expect("load main"), None);
        assert_eq!(journal.load_unresolved(&source).expect("load"), None);

        journal.write(&previous).expect("previous session record");
        let offered = journal
            .load_unresolved(&source)
            .expect("load")
            .expect("record offered")
            .entry;
        let live = RecoveryEntry::new(source.clone(), Revision(1), "live edit".into(), None);
        journal.write(&live).expect("live draft");
        journal.dismiss_pending(&offered).expect("dismiss");
        assert!(!journal.pending_path_for(&source).exists());
        assert_eq!(journal.load(&source).expect("load main"), Some(live));
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[test]
    fn restoring_unresolved_recovery_journals_the_draft_before_release() {
        let directory = temporary_directory("journal-restore");
        let journal = RecoveryJournal::in_directory(directory.clone());
        let source = directory.join("source.md");
        let previous = RecoveryEntry::new(source.clone(), Revision(3), "crash draft".into(), None);
        journal.write(&previous).expect("previous session record");
        let offered = journal
            .load_unresolved(&source)
            .expect("load")
            .expect("record offered")
            .entry;
        journal
            .write(&RecoveryEntry::new(
                source.clone(),
                Revision(1),
                "live edit".into(),
                None,
            ))
            .expect("live draft");

        let restored =
            RecoveryEntry::new(source.clone(), Revision(2), offered.markdown.clone(), None);
        journal
            .restore_pending(&offered, &restored)
            .expect("restore");
        assert!(!journal.pending_path_for(&source).exists());
        assert_eq!(journal.load(&source).expect("load main"), Some(restored));
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[test]
    fn releasing_a_promoted_record_keeps_the_main_draft() {
        let directory = temporary_directory("journal-release");
        let journal = RecoveryJournal::in_directory(directory.clone());
        let source = directory.join("source.md");
        let live = RecoveryEntry::new(source.clone(), Revision(4), "live edit".into(), None);
        journal.write(&live).expect("live draft");
        let offered = journal
            .load_unresolved(&source)
            .expect("load")
            .expect("record offered");
        assert!(offered.promoted);

        journal.release_pending(&offered.entry).expect("release");
        assert!(!journal.pending_path_for(&source).exists());
        assert_eq!(journal.load(&source).expect("load main"), Some(live));

        // A sidecar that no longer holds the released record is kept.
        let other = RecoveryEntry::new(source.clone(), Revision(9), "other".into(), None);
        journal
            .write_unlocked(&journal.pending_path_for(&source), &other)
            .expect("other pending record");
        journal.release_pending(&offered.entry).expect("release");
        assert!(journal.pending_path_for(&source).exists());
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[test]
    fn recovery_journal_round_trips_and_clears() {
        let directory = temporary_directory("journal");
        let journal = RecoveryJournal::in_directory(directory.clone());
        let source = directory.join("source.md");
        let entry = RecoveryEntry::new(source.clone(), Revision(7), "draft".into(), None);
        journal.write(&entry).expect("write journal");
        assert_eq!(journal.load(&source).expect("load"), Some(entry));
        journal.clear(&source).expect("clear");
        assert_eq!(journal.load(&source).expect("load cleared"), None);
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[test]
    fn recovery_records_base_identity_and_revision_safe_cleanup() {
        let directory = temporary_directory("journal-identity");
        let journal = RecoveryJournal::in_directory(directory.clone());
        let source = directory.join("source.md");
        fs::write(&source, "base").expect("source");
        let base = source_identity(&source).expect("identity");
        let newer = RecoveryEntry::new(
            source.clone(),
            Revision(9),
            "newer draft".into(),
            Some(base.clone()),
        );
        journal.write(&newer).expect("write journal");
        journal
            .clear_revision(&source, Revision(8))
            .expect("older cleanup is harmless");
        let loaded = journal
            .load(&source)
            .expect("load")
            .expect("newer record remains");
        assert_eq!(loaded.base_identity, Some(base));
        assert_eq!(loaded.markdown, "newer draft");
        journal
            .clear_revision(&source, Revision(9))
            .expect("matching cleanup");
        assert_eq!(journal.load(&source).expect("load cleared"), None);
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[test]
    fn malformed_recovery_is_reported_and_retained() {
        let directory = temporary_directory("journal-corrupt");
        let journal = RecoveryJournal::in_directory(directory.clone());
        let source = directory.join("source.md");
        let record = journal.path_for(&source);
        fs::write(&record, b"{not valid json").expect("corrupt record");
        assert!(matches!(
            journal.load(&source),
            Err(PersistenceError::Journal(_))
        ));
        assert!(record.exists(), "the corrupt record remains inspectable");
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[test]
    fn copy_save_never_replaces_an_existing_file() {
        let directory = temporary_directory("copy-save");
        let path = directory.join("copy.md");
        let outcome = atomic_write_new(&path, b"first").expect("create copy");
        assert_eq!(outcome.identity, source_identity(&path).expect("identity"));
        assert!(!has_copy_temporary(&directory));
        atomic_write_new(&path, b"second").expect_err("existing copy is protected");
        assert_eq!(fs::read_to_string(&path).expect("copy"), "first");
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[test]
    fn target_writes_verify_the_observed_file_before_replacing_it() {
        let directory = temporary_directory("write-target");
        let path = directory.join("export.html");
        assert_eq!(existing_identity(&path).expect("missing target"), None);
        let created = write_target(&path, None, b"first").expect("create target");
        let observed = existing_identity(&path)
            .expect("observe target")
            .expect("target exists");
        assert_eq!(created.identity, observed);

        let replaced = write_target(&path, Some(&observed), b"second").expect("replace target");
        assert_eq!(
            replaced.identity,
            source_identity(&path).expect("replaced identity")
        );
        let error = write_target(&path, Some(&observed), b"stale")
            .expect_err("a stale observation must not replace the target");
        assert!(matches!(error, PersistenceError::ExternalChange(_)));
        assert_eq!(fs::read_to_string(&path).expect("target"), "second");
        write_target(&path, None, b"late").expect_err("creation never replaces a file");
        assert_eq!(fs::read_to_string(&path).expect("target"), "second");
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    fn has_copy_temporary(directory: &Path) -> bool {
        fs::read_dir(directory).expect("directory").any(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .contains("tachyon-copy")
        })
    }

    #[test]
    fn copy_save_falls_back_when_hard_links_are_unsupported() {
        for kind in [io::ErrorKind::Unsupported, io::ErrorKind::PermissionDenied] {
            let directory = temporary_directory("copy-fallback");
            let path = directory.join("copy.md");
            let outcome = atomic_write_new_using(&path, b"first", |_, _| {
                Err(io::Error::new(kind, "injected missing hard links"))
            })
            .expect("fallback creates the copy");
            assert_eq!(outcome.identity, source_identity(&path).expect("identity"));
            assert_eq!(fs::read_to_string(&path).expect("copy"), "first");
            assert!(!has_copy_temporary(&directory));

            atomic_write_new_using(&path, b"second", |_, _| {
                Err(io::Error::new(kind, "injected missing hard links"))
            })
            .expect_err("fallback never replaces an existing file");
            assert_eq!(fs::read_to_string(&path).expect("copy"), "first");
            assert!(!has_copy_temporary(&directory));
            fs::remove_dir_all(directory).expect("cleanup isolated test directory");
        }
    }

    #[test]
    fn copy_save_reports_other_link_failures_without_fallback() {
        let directory = temporary_directory("copy-link-failure");
        let path = directory.join("copy.md");
        let error = atomic_write_new_using(&path, b"first", |_, _| {
            Err(io::Error::other("injected link failure"))
        })
        .expect_err("unrelated link failures are reported");
        assert!(matches!(error, PersistenceError::Io { path: ref failed, .. } if failed == &path));
        assert!(!path.exists());
        assert!(!has_copy_temporary(&directory));
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[test]
    fn older_workspace_state_defaults_last_open_directory() {
        let state: WorkspaceState =
            serde_json::from_str(r#"{"active_path":"/tmp/notes.md","navigation_width":287.0}"#)
                .expect("older workspace state remains readable");
        assert_eq!(state.last_open_directory, None);
        assert!(!state.justify);
        assert!(!state.hyphenate);
        assert!(!state.navigation_root_explicit);
        assert_eq!(state.active_path, Some(PathBuf::from("/tmp/notes.md")));
        assert_eq!(state.navigation_width, 287.);
    }

    #[test]
    fn workspace_state_round_trips_atomically() {
        let directory = temporary_directory("workspace-state");
        let store = WorkspaceStateStore::at_path(directory.join("workspace.json"));
        assert_eq!(
            store.load().expect("missing state is default"),
            WorkspaceState::default()
        );
        let state = WorkspaceState {
            active_path: Some(directory.join("notes.md")),
            last_open_directory: Some(directory.join("previous-folder")),
            draft_recovery_key: None,
            navigation_root: Some(directory.clone()),
            navigation_root_explicit: true,
            navigation_width: 287.,
            justify: true,
            hyphenate: true,
            expanded_folders: vec![directory.join("archive")],
            selection_start: 7,
            selection_end: 12,
            selection_reversed: true,
            scroll_y: 412.5,
            scroll_anchor_node: Some(NodeId::new_unchecked(42)),
            scroll_anchor_text_hint: "Nearby heading".into(),
            scroll_anchor_text_offset: 3,
            scroll_anchor_projection_offset: 128,
            scroll_anchor_intra_line_offset: 7.5,
        };
        store.write(&state).expect("write state");
        assert_eq!(store.load().expect("load state"), state);
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt as _;
        fs::metadata(path).expect("metadata").permissions().mode() & 0o777
    }

    #[cfg(unix)]
    #[test]
    fn journal_and_workspace_state_are_private() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = temporary_directory("private-state");
        let journal = RecoveryJournal::in_directory(directory.join("recovery"));
        let source = directory.join("source.md");
        journal
            .write(&RecoveryEntry::new(
                source.clone(),
                Revision(1),
                "draft".into(),
                None,
            ))
            .expect("write journal");
        assert_eq!(mode(&journal.path_for(&source)), 0o600);

        let workspace = directory.join("workspace.json");
        fs::write(&workspace, "{}").expect("older workspace state");
        fs::set_permissions(&workspace, fs::Permissions::from_mode(0o644))
            .expect("readable older state");
        WorkspaceStateStore::at_path(workspace.clone())
            .write(&WorkspaceState {
                scroll_anchor_text_hint: "document text".into(),
                ..WorkspaceState::default()
            })
            .expect("write state");
        assert_eq!(mode(&workspace), 0o600);
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[cfg(unix)]
    #[test]
    fn journal_and_workspace_writes_commit_despite_directory_sync_failure() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = temporary_directory("unsynced-state");
        let journal = RecoveryJournal::in_directory(directory.clone());
        let store = WorkspaceStateStore::at_path(directory.join("workspace.json"));
        let source = directory.join("source.md");
        let entry = RecoveryEntry::new(source.clone(), Revision(2), "draft".into(), None);
        let state = WorkspaceState {
            navigation_width: 300.,
            ..WorkspaceState::default()
        };
        // Without read permission the directory cannot be opened to sync it,
        // while entries can still be created and renamed inside it.
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o300))
            .expect("make directory unreadable");
        let sync_blocked = File::open(&directory).is_err();
        let replaced = replace_atomically(
            &directory.join("probe"),
            "probe",
            b"probe",
            &FileMode::Default,
        );
        let journal_written = journal.write(&entry);
        let state_written = store.write(&state);
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .expect("restore directory permissions");

        if sync_blocked {
            assert!(matches!(
                replaced,
                Ok(Some(PersistenceError::DurabilityUncertain { .. }))
            ));
        }
        journal_written.expect("a replaced journal record is committed");
        state_written.expect("replaced workspace state is committed");
        assert_eq!(journal.load(&source).expect("load journal"), Some(entry));
        assert_eq!(store.load().expect("load state"), state);
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }
}
