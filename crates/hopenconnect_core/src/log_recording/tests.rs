use super::*;

fn temp_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "hopenconnect-log-recording-{label}-{}",
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
    assert!(read_archive(&root, "../h-openconnect-2026-01-01.log").is_err());
    assert!(read_archive(&root, "other.log").is_err());
    assert!(delete_archive(&root, "../h-openconnect-2026-01-01.log").is_err());
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

    let old_file_name = "h-openconnect.2000-01-01.log";
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
