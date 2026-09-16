use crate::error::CoreError;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const PLATFORM_VPN_OWNER_JOURNAL_VERSION: u32 = 1;
const MAX_PLATFORM_VPN_OWNER_JOURNAL_BYTES: u64 = 16 * 1024;
const PLATFORM_VPN_OWNER_LEASE_VERSION: u32 = 1;
const MAX_PLATFORM_VPN_OWNER_LEASE_BYTES: u64 = 16 * 1024;
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProcessIdentity {
    pub(crate) boot_id: String,
    pub(crate) pid: u32,
    pub(crate) start_time: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PlatformVpnOwnerPhase {
    Pending,
    Attached,
    /// An exact Stop intent fenced Pending before HarmonyOS Ability stop.
    /// Keep the tombstone until that stop is confirmed so late Wants cannot
    /// attach and pre-stop journal absence is never treated as cleanup proof.
    Stopping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PlatformVpnOwnerJournal {
    pub(crate) attempt_id: String,
    pub(crate) issuer: ProcessIdentity,
    pub(crate) extension: Option<ProcessIdentity>,
    pub(crate) phase: PlatformVpnOwnerPhase,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JournalRead {
    Missing,
    Present(PlatformVpnOwnerJournal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PlatformVpnOwnerLeaseRole {
    Issuer,
    Extension,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PlatformVpnOwnerLeaseRecord {
    pub(crate) attempt_id: String,
    pub(crate) identity: ProcessIdentity,
    pub(crate) role: PlatformVpnOwnerLeaseRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlatformVpnOwnerLeaseObservation {
    HeldExact,
    HeldOther,
    Released,
}

/// An exclusive lease on one fixed inode. Dropping it releases ownership; the
/// inode itself is deliberately never renamed or removed by this module.
#[derive(Debug)]
pub(crate) struct PlatformVpnOwnerLease {
    file: File,
}

impl Drop for PlatformVpnOwnerLease {
    fn drop(&mut self) {
        // Explicit unlock prevents a forked child from extending ownership
        // during the short interval before its O_CLOEXEC descriptors close.
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredPlatformVpnOwnerJournal {
    version: u32,
    attempt_id: String,
    issuer: ProcessIdentity,
    extension: Option<ProcessIdentity>,
    phase: PlatformVpnOwnerPhase,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredPlatformVpnOwnerLeaseRecord {
    version: u32,
    attempt_id: String,
    identity: ProcessIdentity,
    role: PlatformVpnOwnerLeaseRole,
}

impl From<PlatformVpnOwnerJournal> for StoredPlatformVpnOwnerJournal {
    fn from(record: PlatformVpnOwnerJournal) -> Self {
        Self {
            version: PLATFORM_VPN_OWNER_JOURNAL_VERSION,
            attempt_id: record.attempt_id,
            issuer: record.issuer,
            extension: record.extension,
            phase: record.phase,
        }
    }
}

impl TryFrom<StoredPlatformVpnOwnerJournal> for PlatformVpnOwnerJournal {
    type Error = CoreError;

    fn try_from(stored: StoredPlatformVpnOwnerJournal) -> Result<Self, Self::Error> {
        if stored.version != PLATFORM_VPN_OWNER_JOURNAL_VERSION {
            return Err(journal_error(format!(
                "unsupported version {}",
                stored.version
            )));
        }
        let record = Self {
            attempt_id: stored.attempt_id,
            issuer: stored.issuer,
            extension: stored.extension,
            phase: stored.phase,
        };
        validate_record(&record)?;
        Ok(record)
    }
}

impl From<PlatformVpnOwnerLeaseRecord> for StoredPlatformVpnOwnerLeaseRecord {
    fn from(record: PlatformVpnOwnerLeaseRecord) -> Self {
        Self {
            version: PLATFORM_VPN_OWNER_LEASE_VERSION,
            attempt_id: record.attempt_id,
            identity: record.identity,
            role: record.role,
        }
    }
}

impl TryFrom<StoredPlatformVpnOwnerLeaseRecord> for PlatformVpnOwnerLeaseRecord {
    type Error = CoreError;

    fn try_from(stored: StoredPlatformVpnOwnerLeaseRecord) -> Result<Self, Self::Error> {
        if stored.version != PLATFORM_VPN_OWNER_LEASE_VERSION {
            return Err(lease_error(format!(
                "unsupported version {}",
                stored.version
            )));
        }
        let record = Self {
            attempt_id: stored.attempt_id,
            identity: stored.identity,
            role: stored.role,
        };
        validate_lease_record(&record)?;
        Ok(record)
    }
}

struct JournalLock {
    _file: File,
}

impl Drop for JournalLock {
    fn drop(&mut self) {
        // A multi-threaded process can fork while this descriptor is open.
        // Until exec closes inherited descriptors, merely closing our copy
        // can leave the shared open-file description (and its flock) alive in
        // the child. Explicitly unlock before close so an unrelated spawn
        // cannot extend this critical section.
        unsafe {
            libc::flock(self._file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

pub(crate) fn acquire_owner_lease_exact(
    path: &Path,
    record: PlatformVpnOwnerLeaseRecord,
) -> Result<PlatformVpnOwnerLease, CoreError> {
    validate_lease_record(&record)?;
    let file = open_owner_lease(path, true)?;
    if !try_lock_owner_lease(&file, path)? {
        return Err(lease_error(format!(
            "busy: lease '{}' is held by another owner",
            path.display()
        )));
    }
    let mut lease = PlatformVpnOwnerLease { file };
    write_owner_lease_record(&mut lease.file, path, record)?;
    Ok(lease)
}

pub(crate) fn observe_owner_lease_exact(
    path: &Path,
    expected: &PlatformVpnOwnerLeaseRecord,
) -> Result<PlatformVpnOwnerLeaseObservation, CoreError> {
    validate_lease_record(expected)?;
    let mut file = open_owner_lease(path, false)?;
    if try_lock_owner_lease(&file, path)? {
        drop(PlatformVpnOwnerLease { file });
        return Ok(PlatformVpnOwnerLeaseObservation::Released);
    }

    // A busy lock proves only that this fixed inode has a holder. Read the
    // strict record from the same descriptor: stale or partially rewritten
    // content is HeldOther, never proof about the expected owner.
    let observed = read_owner_lease_record(&mut file, path)?;
    Ok(if observed.as_ref() == Some(expected) {
        PlatformVpnOwnerLeaseObservation::HeldExact
    } else {
        PlatformVpnOwnerLeaseObservation::HeldOther
    })
}

/// Hold an already released lease without changing its record. The observer
/// must verify the owner journal while holding this guard; a replacement
/// Extension cannot acquire ownership until the observation is committed.
pub(crate) fn lock_released_owner_lease(
    path: &Path,
) -> Result<Option<PlatformVpnOwnerLease>, CoreError> {
    let file = open_owner_lease(path, false)?;
    if try_lock_owner_lease(&file, path)? {
        Ok(Some(PlatformVpnOwnerLease { file }))
    } else {
        Ok(None)
    }
}

pub(crate) fn read(path: &Path) -> Result<JournalRead, CoreError> {
    let _lock = lock_journal(path)?;
    read_unlocked(path)
}

pub(crate) fn create_pending_exact(
    path: &Path,
    record: PlatformVpnOwnerJournal,
) -> Result<(), CoreError> {
    validate_record(&record)?;
    if record.phase != PlatformVpnOwnerPhase::Pending || record.extension.is_some() {
        return Err(journal_error(
            "a new owner must be Pending without an Extension identity",
        ));
    }

    let _lock = lock_journal(path)?;
    match read_unlocked(path)? {
        JournalRead::Missing => write_unlocked(path, record),
        JournalRead::Present(current) if current == record => Ok(()),
        JournalRead::Present(current) => Err(journal_error(format!(
            "create conflict: existing attempt '{}' is not the requested attempt '{}'",
            current.attempt_id, record.attempt_id
        ))),
    }
}

pub(crate) fn upgrade_attached_exact(
    path: &Path,
    expected_attempt: &str,
    expected_issuer: ProcessIdentity,
    extension: ProcessIdentity,
) -> Result<(), CoreError> {
    validate_attempt_id(expected_attempt)?;
    validate_identity("expected issuer", &expected_issuer)?;
    validate_identity("Extension", &extension)?;

    let _lock = lock_journal(path)?;
    let JournalRead::Present(mut current) = read_unlocked(path)? else {
        return Err(journal_error(format!(
            "cannot attach missing attempt '{expected_attempt}'"
        )));
    };
    if current.attempt_id != expected_attempt || current.issuer != expected_issuer {
        return Err(journal_error(format!(
            "attach conflict: attempt '{expected_attempt}' or its issuer is no longer current"
        )));
    }
    match (current.phase, current.extension.as_ref()) {
        (PlatformVpnOwnerPhase::Pending, None) => {
            current.phase = PlatformVpnOwnerPhase::Attached;
            current.extension = Some(extension);
            write_unlocked(path, current)
        }
        (PlatformVpnOwnerPhase::Attached, Some(current_extension))
            if current_extension == &extension =>
        {
            Ok(())
        }
        _ => Err(journal_error(format!(
            "attach conflict: attempt '{expected_attempt}' has a different phase or Extension owner"
        ))),
    }
}

/// Persistently fence an exact Pending attempt before asking HarmonyOS to
/// stop the Extension Ability. `false` means attachment or replacement won
/// the race and the caller must re-read the journal; it is never cleanup
/// confirmation.
pub(crate) fn fence_pending_stop_exact(
    path: &Path,
    expected_attempt: &str,
    expected_issuer: ProcessIdentity,
) -> Result<bool, CoreError> {
    validate_attempt_id(expected_attempt)?;
    validate_identity("expected issuer", &expected_issuer)?;

    let _lock = lock_journal(path)?;
    let JournalRead::Present(mut current) = read_unlocked(path)? else {
        return Ok(false);
    };
    if current.attempt_id != expected_attempt || current.issuer != expected_issuer {
        return Ok(false);
    }
    match (current.phase, current.extension.as_ref()) {
        (PlatformVpnOwnerPhase::Pending, None) => {
            current.phase = PlatformVpnOwnerPhase::Stopping;
            write_unlocked(path, current)?;
            Ok(true)
        }
        (PlatformVpnOwnerPhase::Stopping, None) => Ok(true),
        (PlatformVpnOwnerPhase::Attached, Some(_)) => Ok(false),
        _ => Err(journal_error(format!(
            "stop fence conflict: attempt '{expected_attempt}' has an invalid phase or Extension owner"
        ))),
    }
}

/// Replace the process identity of an already attached Extension without
/// changing the attempt or its issuer. The caller must prove that
/// `expected_extension` released its exact ownership lease before invoking
/// this compare-and-swap.
pub(crate) fn rebind_attached_exact(
    path: &Path,
    expected_attempt: &str,
    expected_issuer: ProcessIdentity,
    expected_extension: ProcessIdentity,
    replacement_extension: ProcessIdentity,
) -> Result<(), CoreError> {
    validate_attempt_id(expected_attempt)?;
    validate_identity("expected issuer", &expected_issuer)?;
    validate_identity("expected Extension", &expected_extension)?;
    validate_identity("replacement Extension", &replacement_extension)?;

    let _lock = lock_journal(path)?;
    let JournalRead::Present(mut current) = read_unlocked(path)? else {
        return Err(journal_error(format!(
            "cannot rebind missing attempt '{expected_attempt}'"
        )));
    };
    if current.attempt_id != expected_attempt
        || current.issuer != expected_issuer
        || current.phase != PlatformVpnOwnerPhase::Attached
    {
        return Err(journal_error(format!(
            "rebind conflict: attempt '{expected_attempt}' or its issuer is no longer current"
        )));
    }
    match current.extension.as_ref() {
        Some(extension) if extension == &expected_extension => {
            if extension == &replacement_extension {
                return Ok(());
            }
            current.extension = Some(replacement_extension);
            write_unlocked(path, current)
        }
        // A retry of the same completed CAS is idempotent even though the
        // journal no longer contains its former expected identity.
        Some(extension) if extension == &replacement_extension => Ok(()),
        _ => Err(journal_error(format!(
            "rebind conflict: attempt '{expected_attempt}' has a different Extension owner"
        ))),
    }
}

pub(crate) fn delete_exact(
    path: &Path,
    expected_attempt: &str,
    expected_extension: Option<ProcessIdentity>,
) -> Result<bool, CoreError> {
    validate_attempt_id(expected_attempt)?;
    if let Some(extension) = expected_extension.as_ref() {
        validate_identity("expected Extension", extension)?;
    }

    let _lock = lock_journal(path)?;
    let JournalRead::Present(current) = read_unlocked(path)? else {
        return Ok(false);
    };
    // None is an exact expectation, not a wildcard. This makes an unattached
    // cleanup incapable of deleting a record which an Extension has adopted.
    if current.attempt_id != expected_attempt || current.extension != expected_extension {
        return Ok(false);
    }
    fs::remove_file(path)
        .map_err(|error| io_error("remove platform VPN owner journal", path, error))?;
    sync_parent(path)?;
    Ok(true)
}

/// Delete only an exact still-Pending record. Unlike `delete_exact(...,
/// None)`, this cannot remove a durable Stopping tombstone and is therefore
/// safe for a pre-OS-stop dispatch failure.
pub(crate) fn delete_pending_exact(path: &Path, expected_attempt: &str) -> Result<bool, CoreError> {
    validate_attempt_id(expected_attempt)?;

    let _lock = lock_journal(path)?;
    let JournalRead::Present(current) = read_unlocked(path)? else {
        return Ok(false);
    };
    if current.attempt_id != expected_attempt
        || current.phase != PlatformVpnOwnerPhase::Pending
        || current.extension.is_some()
    {
        return Ok(false);
    }
    fs::remove_file(path)
        .map_err(|error| io_error("remove pending platform VPN owner journal", path, error))?;
    sync_parent(path)?;
    Ok(true)
}

fn validate_lease_record(record: &PlatformVpnOwnerLeaseRecord) -> Result<(), CoreError> {
    if record.attempt_id.trim().is_empty() {
        return Err(lease_error("attempt id is empty"));
    }
    if record.identity.boot_id.trim().is_empty()
        || record.identity.pid == 0
        || record.identity.start_time == 0
    {
        return Err(lease_error(
            "holder identity must contain a boot id, non-zero PID, and start time",
        ));
    }
    Ok(())
}

fn open_owner_lease(path: &Path, create: bool) -> Result<File, CoreError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| lease_error(format!("path '{}' has no parent directory", path.display())))?;
    if create {
        fs::create_dir_all(parent).map_err(|error| {
            io_error("create platform VPN owner lease directory", parent, error)
        })?;
    }

    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err(lease_error(format!(
                "path '{}' is not a regular file",
                path.display()
            )))
        }
        Ok(_) => {}
        Err(error) if create && error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error("inspect platform VPN owner lease", path, error)),
    }

    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create(create)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    let file = options
        .open(path)
        .map_err(|error| io_error("open platform VPN owner lease", path, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| io_error("inspect opened platform VPN owner lease", path, error))?;
    if !metadata.is_file() {
        return Err(lease_error(format!(
            "opened path '{}' is not a regular file",
            path.display()
        )));
    }
    Ok(file)
}

fn try_lock_owner_lease(file: &File, path: &Path) -> Result<bool, CoreError> {
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        return Ok(true);
    }
    let error = std::io::Error::last_os_error();
    if error.kind() == std::io::ErrorKind::WouldBlock {
        return Ok(false);
    }
    Err(io_error("lock platform VPN owner lease", path, error))
}

fn write_owner_lease_record(
    file: &mut File,
    path: &Path,
    record: PlatformVpnOwnerLeaseRecord,
) -> Result<(), CoreError> {
    let bytes = serde_json::to_vec(&StoredPlatformVpnOwnerLeaseRecord::from(record))
        .map_err(|error| lease_error(format!("serialize file '{}': {error}", path.display())))?;
    if bytes.len() as u64 > MAX_PLATFORM_VPN_OWNER_LEASE_BYTES {
        return Err(lease_error(format!(
            "serialized file '{}' is too large ({} bytes)",
            path.display(),
            bytes.len()
        )));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| io_error("seek platform VPN owner lease", path, error))?;
    file.set_len(0)
        .map_err(|error| io_error("truncate platform VPN owner lease", path, error))?;
    file.write_all(&bytes)
        .map_err(|error| io_error("write platform VPN owner lease", path, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync platform VPN owner lease", path, error))?;
    sync_parent(path)
}

fn read_owner_lease_record(
    file: &mut File,
    path: &Path,
) -> Result<Option<PlatformVpnOwnerLeaseRecord>, CoreError> {
    file.seek(SeekFrom::Start(0))
        .map_err(|error| io_error("seek platform VPN owner lease", path, error))?;
    let mut bytes = Vec::new();
    (&mut *file)
        .take(MAX_PLATFORM_VPN_OWNER_LEASE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("read platform VPN owner lease", path, error))?;
    if bytes.len() as u64 > MAX_PLATFORM_VPN_OWNER_LEASE_BYTES {
        return Ok(None);
    }
    let Ok(stored) = serde_json::from_slice::<StoredPlatformVpnOwnerLeaseRecord>(&bytes) else {
        return Ok(None);
    };
    Ok(PlatformVpnOwnerLeaseRecord::try_from(stored).ok())
}

fn validate_record(record: &PlatformVpnOwnerJournal) -> Result<(), CoreError> {
    validate_attempt_id(&record.attempt_id)?;
    validate_identity("issuer", &record.issuer)?;
    if let Some(extension) = record.extension.as_ref() {
        validate_identity("Extension", extension)?;
    }
    match (record.phase, record.extension.as_ref()) {
        (PlatformVpnOwnerPhase::Pending, None)
        | (PlatformVpnOwnerPhase::Attached, Some(_))
        | (PlatformVpnOwnerPhase::Stopping, None) => Ok(()),
        (PlatformVpnOwnerPhase::Pending, Some(_)) => Err(journal_error(
            "Pending owner unexpectedly contains an Extension identity",
        )),
        (PlatformVpnOwnerPhase::Attached, None) => Err(journal_error(
            "Attached owner is missing its Extension identity",
        )),
        (PlatformVpnOwnerPhase::Stopping, Some(_)) => Err(journal_error(
            "Stopping owner unexpectedly contains an Extension identity",
        )),
    }
}

fn validate_attempt_id(attempt_id: &str) -> Result<(), CoreError> {
    if attempt_id.trim().is_empty() {
        return Err(journal_error("attempt id is empty"));
    }
    Ok(())
}

fn validate_identity(label: &str, identity: &ProcessIdentity) -> Result<(), CoreError> {
    if identity.boot_id.trim().is_empty() || identity.pid == 0 || identity.start_time == 0 {
        return Err(journal_error(format!(
            "{label} process identity must contain a boot id, non-zero PID, and start time"
        )));
    }
    Ok(())
}

fn lock_journal(path: &Path) -> Result<JournalLock, CoreError> {
    let parent = journal_parent(path)?;
    fs::create_dir_all(parent)
        .map_err(|error| io_error("create platform VPN journal directory", parent, error))?;
    let lock_path = lock_path(path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC)
        .open(&lock_path)
        .map_err(|error| io_error("open platform VPN owner journal lock", &lock_path, error))?;
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::WouldBlock {
            return Err(journal_error(format!(
                "busy: lock '{}' is held by another process",
                lock_path.display()
            )));
        }
        return Err(io_error(
            "lock platform VPN owner journal",
            &lock_path,
            error,
        ));
    }
    Ok(JournalLock { _file: file })
}

fn read_unlocked(path: &Path) -> Result<JournalRead, CoreError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(JournalRead::Missing)
        }
        Err(error) => return Err(io_error("inspect platform VPN owner journal", path, error)),
    };
    if !metadata.is_file() {
        return Err(journal_error(format!(
            "path '{}' is not a regular file",
            path.display()
        )));
    }
    if metadata.len() > MAX_PLATFORM_VPN_OWNER_JOURNAL_BYTES {
        return Err(journal_error(format!(
            "file '{}' is too large ({} bytes)",
            path.display(),
            metadata.len()
        )));
    }
    let bytes =
        fs::read(path).map_err(|error| io_error("read platform VPN owner journal", path, error))?;
    let stored: StoredPlatformVpnOwnerJournal = serde_json::from_slice(&bytes)
        .map_err(|error| journal_error(format!("malformed file '{}': {error}", path.display())))?;
    Ok(JournalRead::Present(stored.try_into()?))
}

fn write_unlocked(path: &Path, record: PlatformVpnOwnerJournal) -> Result<(), CoreError> {
    validate_record(&record)?;
    let bytes = serde_json::to_vec(&StoredPlatformVpnOwnerJournal::from(record))
        .map_err(|error| journal_error(format!("serialize file '{}': {error}", path.display())))?;
    let temp_path = unique_temp_path(path)?;
    let result = (|| -> Result<(), CoreError> {
        let mut temp = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp_path)
            .map_err(|error| {
                io_error(
                    "create temporary platform VPN owner journal",
                    &temp_path,
                    error,
                )
            })?;
        temp.write_all(&bytes).map_err(|error| {
            io_error(
                "write temporary platform VPN owner journal",
                &temp_path,
                error,
            )
        })?;
        temp.sync_all().map_err(|error| {
            io_error(
                "sync temporary platform VPN owner journal",
                &temp_path,
                error,
            )
        })?;
        fs::rename(&temp_path, path)
            .map_err(|error| io_error("replace platform VPN owner journal", path, error))?;
        sync_parent(path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn sync_parent(path: &Path) -> Result<(), CoreError> {
    let parent = journal_parent(path)?;
    let directory = File::open(parent)
        .map_err(|error| io_error("open platform VPN journal directory", parent, error))?;
    directory
        .sync_all()
        .map_err(|error| io_error("sync platform VPN journal directory", parent, error))
}

fn journal_parent(path: &Path) -> Result<&Path, CoreError> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| journal_error(format!("path '{}' has no parent directory", path.display())))
}

fn lock_path(path: &Path) -> Result<PathBuf, CoreError> {
    let file_name = path
        .file_name()
        .ok_or_else(|| journal_error(format!("path '{}' has no file name", path.display())))?;
    let mut lock_name = OsString::from(file_name);
    lock_name.push(".lock");
    Ok(path.with_file_name(lock_name))
}

fn unique_temp_path(path: &Path) -> Result<PathBuf, CoreError> {
    let file_name = path
        .file_name()
        .ok_or_else(|| journal_error(format!("path '{}' has no file name", path.display())))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut temp_name = OsString::from(".");
    temp_name.push(file_name);
    temp_name.push(format!(
        ".tmp-{}-{timestamp}-{sequence}",
        std::process::id()
    ));
    Ok(path.with_file_name(temp_name))
}

fn journal_error(message: impl Into<String>) -> CoreError {
    CoreError::msg(format!(
        "invalid platform VPN owner journal: {}",
        message.into()
    ))
}

fn lease_error(message: impl Into<String>) -> CoreError {
    CoreError::msg(format!(
        "invalid platform VPN owner lease: {}",
        message.into()
    ))
}

fn io_error(context: &str, path: &Path, error: std::io::Error) -> CoreError {
    CoreError::msg(format!("{context} '{}': {error}", path.display()))
}

pub(crate) fn current_process_identity() -> Result<ProcessIdentity, CoreError> {
    let pid = std::process::id();
    let start_time = read_process_start_time(pid).map_err(|error| {
        CoreError::msg(format!(
            "cannot read current process start identity for PID {pid}: {error}"
        ))
    })?;
    let boot_id = read_boot_id().map_err(|error| {
        CoreError::msg(format!("cannot read current system boot identity: {error}"))
    })?;
    if start_time == 0 || boot_id.is_empty() {
        return Err(CoreError::msg("current process identity is incomplete"));
    }
    Ok(ProcessIdentity {
        pid,
        start_time,
        boot_id,
    })
}

fn read_process_start_time(pid: u32) -> Result<u64, std::io::Error> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let command_end = stat.rfind(')').ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "missing process command terminator",
        )
    })?;
    // Fields after the command start at process state (field 3); starttime is
    // field 22, hence zero-based index 19 in this suffix. Pairing it with PID
    // fences cleanup recovery against PID reuse.
    stat.get(command_end + 1..)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid process stat suffix",
            )
        })?
        .split_whitespace()
        .nth(19)
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "missing process start time",
            )
        })?
        .parse()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn read_boot_id() -> Result<String, std::io::Error> {
    let boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    let boot_id = boot_id.trim();
    if boot_id.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "system boot identity is empty",
        ));
    }
    Ok(boot_id.to_owned())
}
