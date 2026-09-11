use std::{
    ffi::OsStr,
    fmt::Write as _,
    fs::{self, File, OpenOptions},
    io::{self, Read as _, Write as _},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use document_core::{NodeId, Revision, SaveSnapshot, SourceIdentity};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

static SAVE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalState {
    Unchanged,
    Modified(SourceIdentity),
    Deleted,
}

pub fn source_identity(path: &Path) -> Result<SourceIdentity, PersistenceError> {
    read_source_bytes_with_identity(path).map(|(_, identity)| identity)
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
    let content_hash: [u8; 32] = Sha256::digest(&bytes).into();
    let identity = SourceIdentity {
        path: path.to_path_buf(),
        length: after.len(),
        modified: after.modified().ok(),
        content_hash,
        #[cfg(unix)]
        device: after.dev(),
        #[cfg(unix)]
        inode: after.ino(),
    };
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

pub fn atomic_save(snapshot: SaveSnapshot) -> Result<SourceIdentity, PersistenceError> {
    atomic_save_with_outcome(snapshot).map(|outcome| outcome.identity)
}

pub(crate) fn atomic_save_with_outcome(
    snapshot: SaveSnapshot,
) -> Result<WriteOutcome, PersistenceError> {
    atomic_save_with_hook(snapshot, |_, _| Ok(()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AtomicSaveStage {
    CreateTemporary,
    Write,
    SyncTemporary,
    Replace,
    SyncDirectory,
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

fn atomic_save_with_hook(
    snapshot: SaveSnapshot,
    mut before: impl FnMut(AtomicSaveStage, &Path) -> io::Result<()>,
) -> Result<WriteOutcome, PersistenceError> {
    let path = snapshot
        .expected_identity
        .as_ref()
        .map(|identity| identity.path.clone())
        .ok_or_else(|| PersistenceError::Journal("save snapshot has no target path".into()))?;
    if let Some(expected) = snapshot.expected_identity.as_ref() {
        require_expected_identity(&path, expected)?;
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .unwrap_or_else(|| OsStr::new("document.md"));
    let sequence = SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{}.mineral-save-{}-{sequence}",
        filename.to_string_lossy(),
        std::process::id()
    ));
    let original_permissions = fs::metadata(&path)
        .ok()
        .map(|metadata| metadata.permissions());

    let result = (|| {
        before(AtomicSaveStage::CreateTemporary, &temporary)
            .map_err(|error| PersistenceError::io(&temporary, error))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| PersistenceError::io(&temporary, error))?;
        if let Some(permissions) = original_permissions {
            output
                .set_permissions(permissions)
                .map_err(|error| PersistenceError::io(&temporary, error))?;
        }
        before(AtomicSaveStage::Write, &temporary)
            .map_err(|error| PersistenceError::io(&temporary, error))?;
        output
            .write_all(&snapshot.bytes)
            .map_err(|error| PersistenceError::io(&temporary, error))?;
        before(AtomicSaveStage::SyncTemporary, &temporary)
            .map_err(|error| PersistenceError::io(&temporary, error))?;
        output
            .sync_all()
            .map_err(|error| PersistenceError::io(&temporary, error))?;
        drop(output);
        before(AtomicSaveStage::Replace, &path)
            .map_err(|error| PersistenceError::io(&path, error))?;
        if let Some(expected) = snapshot.expected_identity.as_ref() {
            require_expected_identity(&path, expected)?;
        }
        fs::rename(&temporary, &path).map_err(|error| PersistenceError::io(&path, error))?;
        let identity = source_identity(&path)?;
        let sync_result = before(AtomicSaveStage::SyncDirectory, parent)
            .and_then(|()| File::open(parent).and_then(|directory| directory.sync_all()));
        let durability_warning =
            sync_result
                .err()
                .map(|source| PersistenceError::DurabilityUncertain {
                    path: path.clone(),
                    source,
                });
        Ok(WriteOutcome {
            identity,
            durability_warning,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn save_with_recovery(
    snapshot: SaveSnapshot,
    journal: &RecoveryJournal,
) -> Result<SaveOutcome, RecoverableSaveError> {
    save_with_recovery_using(snapshot, journal, atomic_save_with_outcome)
}

fn save_with_recovery_using(
    snapshot: SaveSnapshot,
    journal: &RecoveryJournal,
    save: impl FnOnce(SaveSnapshot) -> Result<WriteOutcome, PersistenceError>,
) -> Result<SaveOutcome, RecoverableSaveError> {
    let Some(source_path) = snapshot
        .expected_identity
        .as_ref()
        .map(|identity| identity.path.clone())
    else {
        return Err(RecoverableSaveError::Unrecovered {
            save: PersistenceError::Journal("save snapshot has no target path".into()),
            recovery: PersistenceError::Journal(
                "recovery cannot be keyed without a target path".into(),
            ),
        });
    };
    let entry = RecoveryEntry::new(
        source_path.clone(),
        snapshot.revision,
        String::from_utf8_lossy(&snapshot.bytes).into_owned(),
        snapshot.expected_identity.clone(),
    );
    let recovery_error = journal.write(&entry).err();

    match save(snapshot) {
        Ok(outcome) => {
            let cleanup_warning = match outcome.durability_warning {
                Some(warning) => Some(warning),
                None => journal.clear(&source_path).err(),
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
pub fn atomic_write_new(path: &Path, bytes: &[u8]) -> Result<SourceIdentity, PersistenceError> {
    atomic_write_new_with_outcome(path, bytes).map(|outcome| outcome.identity)
}

pub(crate) fn atomic_write_new_with_outcome(
    path: &Path,
    bytes: &[u8],
) -> Result<WriteOutcome, PersistenceError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| PersistenceError::io(parent, error))?;
    let filename = path
        .file_name()
        .unwrap_or_else(|| OsStr::new("document.md"));
    let sequence = SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{}.mineral-copy-{}-{sequence}",
        filename.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| PersistenceError::io(&temporary, error))?;
        output
            .write_all(bytes)
            .map_err(|error| PersistenceError::io(&temporary, error))?;
        output
            .sync_all()
            .map_err(|error| PersistenceError::io(&temporary, error))?;
        drop(output);
        fs::hard_link(&temporary, path).map_err(|error| PersistenceError::io(path, error))?;
        let identity = source_identity(path)?;
        let cleanup_warning = fs::remove_file(&temporary)
            .err()
            .map(|error| PersistenceError::io(&temporary, error));
        let durability_warning = File::open(parent)
            .and_then(|directory| directory.sync_all())
            .err()
            .map(|source| PersistenceError::DurabilityUncertain {
                path: path.to_path_buf(),
                source,
            });
        Ok(WriteOutcome {
            identity,
            durability_warning: durability_warning.or(cleanup_warning),
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
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

#[derive(Clone, Debug)]
pub struct RecoveryJournal {
    directory: PathBuf,
}

impl RecoveryJournal {
    #[must_use]
    pub fn for_current_user() -> Self {
        Self {
            directory: state_root().join("mineral-markdown/recovery"),
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
        fs::create_dir_all(&self.directory)
            .map_err(|error| PersistenceError::io(&self.directory, error))?;
        let target = self.path_for(&entry.source_path);
        let temporary = target.with_extension(format!(
            "journal-{}-{}",
            std::process::id(),
            SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let bytes = serde_json::to_vec(entry)
            .map_err(|error| PersistenceError::Journal(error.to_string()))?;
        let result = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                options.mode(0o600);
            }
            let mut file = options
                .open(&temporary)
                .map_err(|error| PersistenceError::io(&temporary, error))?;
            file.write_all(&bytes)
                .map_err(|error| PersistenceError::io(&temporary, error))?;
            file.sync_all()
                .map_err(|error| PersistenceError::io(&temporary, error))?;
            drop(file);
            fs::rename(&temporary, &target)
                .map_err(|error| PersistenceError::io(&target, error))?;
            sync_directory(&self.directory)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    pub fn load(&self, source_path: &Path) -> Result<Option<RecoveryEntry>, PersistenceError> {
        let _gate = RECOVERY_GATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.load_unlocked(source_path)
    }

    fn load_unlocked(&self, source_path: &Path) -> Result<Option<RecoveryEntry>, PersistenceError> {
        let path = self.path_for(source_path);
        match fs::read(&path) {
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
        let path = self.path_for(source_path);
        match fs::remove_file(&path) {
            Ok(()) => sync_directory(&self.directory),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(PersistenceError::io(path, error)),
        }
    }

    fn path_for(&self, source_path: &Path) -> PathBuf {
        let mut hasher = Sha256::new();
        hasher.update(b"mineral-recovery-v1\0");
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
        self.directory.join(format!("v1-{key}.json"))
    }
}

fn sync_directory(directory: &Path) -> Result<(), PersistenceError> {
    File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| PersistenceError::io(directory, error))
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
    pub navigation_width: f32,
    // The old Files-first ratio does not describe the new content-sized Outline.
    #[serde(
        rename = "outline_height_limit",
        default = "default_outline_height_limit"
    )]
    pub navigation_split: f32,
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

fn default_outline_height_limit() -> f32 {
    1.
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            active_path: None,
            last_open_directory: None,
            draft_recovery_key: None,
            navigation_root: None,
            navigation_width: 224.,
            navigation_split: 1.,
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
            path: state_root().join("mineral-markdown/workspace.json"),
        }
    }

    #[cfg(test)]
    fn at_path(path: PathBuf) -> Self {
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

    pub fn write(&self, state: &WorkspaceState) -> Result<(), PersistenceError> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| PersistenceError::io(parent, error))?;
        let temporary = self.path.with_extension(format!(
            "json-{}-{}",
            std::process::id(),
            SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let bytes = serde_json::to_vec(state)
            .map_err(|error| PersistenceError::Journal(error.to_string()))?;
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|error| PersistenceError::io(&temporary, error))?;
            file.write_all(&bytes)
                .map_err(|error| PersistenceError::io(&temporary, error))?;
            file.sync_all()
                .map_err(|error| PersistenceError::io(&temporary, error))?;
            drop(file);
            fs::rename(&temporary, &self.path)
                .map_err(|error| PersistenceError::io(&self.path, error))?;
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| PersistenceError::io(parent, error))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory(label: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "mineral-markdown-{label}-{}-{}",
            std::process::id(),
            SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
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
            expected_identity: Some(identity.clone()),
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
            expected_identity: Some(identity),
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
            expected_identity: Some(source_identity(&path).expect("identity")),
        };

        let error = save_with_recovery_using(snapshot, &journal, |snapshot| {
            atomic_save_with_hook(snapshot, |stage, _| {
                if stage == AtomicSaveStage::Write {
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
                .contains("mineral-save")
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
            expected_identity: Some(source_identity(&path).expect("identity")),
        };

        let error = atomic_save_with_hook(snapshot, |stage, target| {
            if stage == AtomicSaveStage::Replace {
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
            expected_identity: Some(source_identity(&path).expect("identity")),
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
            expected_identity: Some(source_identity(&path).expect("identity")),
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
            atomic_save_with_outcome(snapshot)
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
            expected_identity: Some(source_identity(&path).expect("identity")),
        };

        let outcome = save_with_recovery_using(snapshot, &journal, |snapshot| {
            atomic_save_with_hook(snapshot, |stage, _| {
                if stage == AtomicSaveStage::SyncDirectory {
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
            AtomicSaveStage::CreateTemporary,
            AtomicSaveStage::Write,
            AtomicSaveStage::SyncTemporary,
            AtomicSaveStage::Replace,
        ] {
            let directory = temporary_directory("pre-replace-failure");
            let recovery_directory = temporary_directory("pre-replace-recovery");
            let journal = RecoveryJournal::in_directory(recovery_directory.clone());
            let path = directory.join("document.md");
            fs::write(&path, "old").expect("fixture");
            let snapshot = SaveSnapshot {
                revision: Revision(13),
                bytes: b"uncommitted draft".as_slice().into(),
                expected_identity: Some(source_identity(&path).expect("identity")),
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
                    .contains("mineral-save")
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
            expected_identity: Some(source_identity(&path).expect("identity")),
        };

        let outcome = save_with_recovery_using(snapshot, &journal, |snapshot| {
            let outcome = atomic_save_with_outcome(snapshot)?;
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
        atomic_write_new(&path, b"first").expect("create copy");
        atomic_write_new(&path, b"second").expect_err("existing copy is protected");
        assert_eq!(fs::read_to_string(&path).expect("copy"), "first");
        fs::remove_dir_all(directory).expect("cleanup isolated test directory");
    }

    #[test]
    fn older_workspace_state_defaults_last_open_directory() {
        let state: WorkspaceState =
            serde_json::from_str(r#"{"active_path":"/tmp/notes.md","navigation_width":287.0}"#)
                .expect("older workspace state remains readable");
        assert_eq!(state.last_open_directory, None);
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
            navigation_width: 287.,
            navigation_split: 0.67,
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
}
