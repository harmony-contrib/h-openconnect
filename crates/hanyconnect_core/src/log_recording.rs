use crate::error::{CoreError, CoreResult};
use crate::model::DiagnosticEntry;
use crate::private_fs::{ensure_private_dir, secure_existing_file, write_atomic_private};
use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing_appender::rolling::{RollingFileAppender, Rotation};

pub(crate) const MAX_IN_MEMORY_LOGS: usize = 256;
const MAX_PENDING_RUNTIME_LOGS: usize = 4096;
const LOG_DIRECTORY: &str = "logs";
const RECORDING_MARKER: &str = ".recording";
const LOG_FILE_PREFIX: &str = "h-anyconnect.";
const LEGACY_LOG_FILE_PREFIX: &str = "h-anyconnect-";
const LOG_FILE_SUFFIX: &str = ".log";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogArchiveSummary {
    pub file_name: String,
    pub date: String,
    pub bytes: u64,
    pub updated_at: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogRecordingStatus {
    pub enabled: bool,
    pub archives: Vec<LogArchiveSummary>,
}

pub(crate) struct RecordedLogBuffer {
    root: PathBuf,
    session_id: Option<String>,
    appender: Option<RollingFileAppender>,
    entries: Vec<DiagnosticEntry>,
}

impl RecordedLogBuffer {
    pub(crate) fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            session_id: None,
            appender: None,
            entries: Vec::with_capacity(MAX_IN_MEMORY_LOGS),
        }
    }

    pub(crate) fn push(&mut self, entry: DiagnosticEntry) {
        self.sync_session();
        if self.session_id.is_none() {
            return;
        }
        if let Some(appender) = self.appender.as_mut() {
            let _ = write_log_entry(appender, &entry);
        }
        if self.entries.len() >= MAX_IN_MEMORY_LOGS {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn sync_session(&mut self) {
        let session_id = active_session_id(&self.root);
        if session_id != self.session_id {
            self.entries.clear();
            self.appender = session_id
                .as_ref()
                .and_then(|_| build_daily_appender(&self.root).ok());
            self.session_id = session_id;
        }
    }

    pub(crate) fn enabled(&self) -> bool {
        self.session_id.is_some()
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

impl Deref for RecordedLogBuffer {
    type Target = [DiagnosticEntry];

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

struct CapturedRuntimeLog {
    sequence: u64,
    captured_at: u128,
    entry: DiagnosticEntry,
}

pub(crate) struct RuntimeLogBuffer {
    session_id: Option<String>,
    appender: Option<RollingFileAppender>,
    entries: VecDeque<CapturedRuntimeLog>,
    next_sequence: u64,
    persisted_sequence: u64,
}

impl Default for RuntimeLogBuffer {
    fn default() -> Self {
        Self {
            session_id: None,
            appender: None,
            entries: VecDeque::with_capacity(MAX_IN_MEMORY_LOGS),
            next_sequence: 1,
            persisted_sequence: 0,
        }
    }
}

impl RuntimeLogBuffer {
    pub(crate) fn capture(&mut self, entry: DiagnosticEntry) {
        if self.entries.len() >= MAX_PENDING_RUNTIME_LOGS {
            self.entries.pop_front();
        }
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.entries.push_back(CapturedRuntimeLog {
            sequence,
            captured_at: now_unix_nanos(),
            entry,
        });
    }

    pub(crate) fn sync(&mut self, root: &Path) {
        let session_id = active_session_id(root);
        if session_id != self.session_id {
            let started_at = session_id
                .as_deref()
                .and_then(|value| value.parse::<u128>().ok())
                .unwrap_or(u128::MAX);
            self.entries
                .retain(|captured| captured.captured_at >= started_at);
            self.persisted_sequence = 0;
            self.appender = session_id
                .as_ref()
                .and_then(|_| build_daily_appender(root).ok());
            self.session_id = session_id;
        }
        if self.session_id.is_none() {
            self.entries.clear();
            self.persisted_sequence = 0;
            self.appender = None;
            return;
        }

        let mut persisted_sequence = self.persisted_sequence;
        let Some(appender) = self.appender.as_mut() else {
            return;
        };
        for captured in self
            .entries
            .iter()
            .filter(|captured| captured.sequence > self.persisted_sequence)
        {
            if write_log_entry(appender, &captured.entry).is_ok() {
                persisted_sequence = captured.sequence;
            } else {
                break;
            }
        }
        self.persisted_sequence = persisted_sequence;
        while self.entries.len() > MAX_IN_MEMORY_LOGS
            && self
                .entries
                .front()
                .is_some_and(|captured| captured.sequence <= self.persisted_sequence)
        {
            self.entries.pop_front();
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.persisted_sequence = 0;
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = &DiagnosticEntry> {
        self.entries.iter().map(|captured| &captured.entry)
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

pub(crate) fn reset_recording(root: &Path) -> CoreResult<()> {
    let marker = recording_marker(root);
    if marker.exists() {
        fs::remove_file(marker)?;
    }
    Ok(())
}

pub(crate) fn set_recording_enabled(root: &Path, enabled: bool) -> CoreResult<()> {
    let directory = log_directory(root);
    ensure_private_dir(&directory)?;
    let marker = recording_marker(root);
    if enabled {
        build_daily_appender(root)?;
        let session_id = now_unix_nanos().to_string();
        let temporary_marker = directory.join(format!(
            "{RECORDING_MARKER}.tmp-{}-{}",
            std::process::id(),
            now_unix_nanos()
        ));
        write_atomic_private(&temporary_marker, session_id.as_bytes())?;
        if let Err(error) = fs::rename(&temporary_marker, &marker) {
            let _ = fs::remove_file(temporary_marker);
            return Err(error.into());
        }
    } else if marker.exists() {
        fs::remove_file(marker)?;
    }
    Ok(())
}

pub(crate) fn recording_status(root: &Path) -> CoreResult<LogRecordingStatus> {
    let enabled = active_session_id(root).is_some();
    let active_file_name = enabled.then(current_archive_file_name);
    let mut archives = list_archives(root)?;
    for archive in &mut archives {
        archive.active = active_file_name.as_deref() == Some(archive.file_name.as_str());
    }
    Ok(LogRecordingStatus { enabled, archives })
}

pub(crate) fn read_archive(root: &Path, file_name: &str) -> CoreResult<String> {
    if !is_log_file_name(file_name) {
        return Err(CoreError::msg("invalid log archive name"));
    }
    Ok(fs::read_to_string(log_directory(root).join(file_name))?)
}

pub(crate) fn delete_archive(root: &Path, file_name: &str) -> CoreResult<()> {
    if !is_log_file_name(file_name) {
        return Err(CoreError::msg("invalid log archive name"));
    }
    if active_session_id(root).is_some() && file_name == current_archive_file_name() {
        return Err(CoreError::msg(
            "stop log recording before deleting the active archive",
        ));
    }
    fs::remove_file(log_directory(root).join(file_name))?;
    Ok(())
}

fn list_archives(root: &Path) -> CoreResult<Vec<LogArchiveSummary>> {
    let directory = log_directory(root);
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut archives = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !is_log_file_name(&file_name) {
            continue;
        }
        let metadata = entry.metadata()?;
        if !metadata.is_file() {
            continue;
        }
        archives.push(LogArchiveSummary {
            date: log_file_date(&file_name).unwrap_or_default().to_owned(),
            file_name,
            bytes: metadata.len(),
            updated_at: metadata.modified().ok().and_then(system_time_secs),
            active: false,
        });
    }
    archives.sort_by(|left, right| {
        right
            .date
            .cmp(&left.date)
            .then_with(|| right.file_name.cmp(&left.file_name))
    });
    Ok(archives)
}

fn build_daily_appender(root: &Path) -> CoreResult<RollingFileAppender> {
    let directory = log_directory(root);
    ensure_private_dir(&directory)?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("h-anyconnect")
        .filename_suffix("log")
        .build(&directory)
        .map_err(|error| {
            CoreError::msg(format!("failed to initialize daily log appender: {error}"))
        })?;
    let path = directory.join(current_archive_file_name());
    if path.exists() {
        secure_existing_file(&path)?;
    }
    Ok(appender)
}

fn write_log_entry(appender: &mut RollingFileAppender, entry: &DiagnosticEntry) -> CoreResult<()> {
    let level = entry.level.to_ascii_uppercase();
    let message = entry
        .message
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace(['\r', '\n'], "\\n");
    let line = format!("{}\t{level}\t{message}\n", entry.timestamp);
    appender.write_all(line.as_bytes())?;
    Ok(())
}

fn active_session_id(root: &Path) -> Option<String> {
    fs::read_to_string(recording_marker(root))
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn log_directory(root: &Path) -> PathBuf {
    root.join(LOG_DIRECTORY)
}

fn recording_marker(root: &Path) -> PathBuf {
    log_directory(root).join(RECORDING_MARKER)
}

fn is_log_file_name(file_name: &str) -> bool {
    let Some(date) = log_file_date(file_name) else {
        return false;
    };
    date.len() == 10
        && date.chars().enumerate().all(|(index, value)| match index {
            4 | 7 => value == '-',
            _ => value.is_ascii_digit(),
        })
}

fn log_file_date(file_name: &str) -> Option<&str> {
    [LOG_FILE_PREFIX, LEGACY_LOG_FILE_PREFIX]
        .into_iter()
        .find_map(|prefix| {
            file_name
                .strip_prefix(prefix)
                .and_then(|value| value.strip_suffix(LOG_FILE_SUFFIX))
        })
}

fn current_archive_file_name() -> String {
    let date = time::OffsetDateTime::now_utc().date();
    format!(
        "h-anyconnect.{:04}-{:02}-{:02}.log",
        date.year(),
        date.month() as u8,
        date.day()
    )
}

#[cfg(test)]
fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn now_unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn system_time_secs(time: SystemTime) -> Option<String> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "hanyconnect-log-recording-{label}-{}",
            now_unix_nanos()
        ))
    }

    fn entry(level: &str, message: &str, timestamp: &str) -> DiagnosticEntry {
        DiagnosticEntry {
            level: level.to_owned(),
            message: message.to_owned(),
            timestamp: timestamp.to_owned(),
        }
    }

    #[test]
    fn recording_is_disabled_until_explicitly_enabled() {
        let root = temp_root("disabled");
        let mut logs = RecordedLogBuffer::new(&root);
        logs.push(entry("info", "before enable", "0"));
        assert!(logs.is_empty());
        assert!(!recording_status(&root).unwrap().enabled);
        assert!(recording_status(&root).unwrap().archives.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn enabled_recording_uses_daily_archives() {
        let root = temp_root("daily");
        set_recording_enabled(&root, true).unwrap();
        let mut logs = RecordedLogBuffer::new(&root);
        logs.push(entry("info", "first event", "0"));
        logs.push(entry("warning", "second event", "1"));

        let status = recording_status(&root).unwrap();
        assert!(status.enabled);
        assert_eq!(status.archives.len(), 1);
        assert!(status.archives[0].file_name.starts_with(LOG_FILE_PREFIX));
        let content = read_archive(&root, &status.archives[0].file_name).unwrap();
        assert!(content.contains("first event"));
        assert!(content.contains("second event"));

        set_recording_enabled(&root, false).unwrap();
        logs.push(entry("error", "after disable", "2"));
        let status = recording_status(&root).unwrap();
        assert!(!status.enabled);
        assert!(status.archives.iter().all(|archive| {
            !read_archive(&root, &archive.file_name)
                .unwrap()
                .contains("after disable")
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn archive_names_cannot_escape_the_log_directory() {
        let root = temp_root("safe-name");
        assert!(read_archive(&root, "../h-anyconnect-2026-01-01.log").is_err());
        assert!(read_archive(&root, "other.log").is_err());
        assert!(delete_archive(&root, "../h-anyconnect-2026-01-01.log").is_err());
        assert!(delete_archive(&root, "other.log").is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn active_archive_requires_recording_to_stop_before_deletion() {
        let root = temp_root("delete-active");
        set_recording_enabled(&root, true).unwrap();
        let mut logs = RecordedLogBuffer::new(&root);
        logs.push(entry("info", "delete me", "0"));

        let status = recording_status(&root).unwrap();
        assert_eq!(status.archives.len(), 1);
        assert!(status.archives[0].active);
        let file_name = status.archives[0].file_name.clone();
        assert!(delete_archive(&root, &file_name).is_err());

        let old_file_name = "h-anyconnect.2000-01-01.log";
        fs::write(log_directory(&root).join(old_file_name), "old log").unwrap();
        delete_archive(&root, old_file_name).unwrap();
        assert!(!log_directory(&root).join(old_file_name).exists());

        set_recording_enabled(&root, false).unwrap();
        delete_archive(&root, &file_name).unwrap();
        assert!(recording_status(&root).unwrap().archives.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_file_persists_bursts_beyond_the_ui_history_limit() {
        let root = temp_root("runtime-burst");
        set_recording_enabled(&root, true).unwrap();
        let mut logs = RuntimeLogBuffer::default();
        for index in 0..300 {
            logs.capture(entry(
                "debug",
                &format!("runtime event {index}"),
                &now_unix_seconds().to_string(),
            ));
        }
        logs.sync(&root);

        assert_eq!(logs.len(), MAX_IN_MEMORY_LOGS);
        let status = recording_status(&root).unwrap();
        assert_eq!(status.archives.len(), 1);
        let content = read_archive(&root, &status.archives[0].file_name).unwrap();
        assert_eq!(content.lines().count(), 300);
        let _ = fs::remove_dir_all(root);
    }
}
