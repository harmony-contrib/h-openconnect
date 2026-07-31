const ENGINE: &str = include_str!("../src/engine.rs");
const ENGINE_CONNECTION: &str = include_str!("../src/engine/connection.rs");
const ENGINE_PLATFORM: &str = include_str!("../src/engine/platform.rs");
const PLATFORM_BROWSER: &str = include_str!("../src/platform_browser.rs");
const PLATFORM_IPC: &str = include_str!("../src/platform_ipc.rs");
const PLATFORM_STATE: &str = include_str!("../src/platform_state.rs");

fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start = source.find(start).expect("section start");
    let tail = &source[start..];
    let end = tail.find(end).expect("section end");
    &tail[..end]
}

#[test]
fn runtime_cross_process_payloads_never_use_json_files() {
    let engine_sources = [ENGINE, ENGINE_CONNECTION, ENGINE_PLATFORM].join("\n");
    let transport_sources = [
        ENGINE_CONNECTION,
        ENGINE_PLATFORM,
        PLATFORM_BROWSER,
        PLATFORM_IPC,
        PLATFORM_STATE,
    ]
    .join("\n");
    assert!(!transport_sources.contains("session-handoff.json"));
    assert!(!transport_sources.contains("browser-request.json"));
    assert!(!transport_sources.contains("platform-vpn-state.json"));
    assert!(!engine_sources.contains("read_to_string"));
    assert!(!engine_sources.contains("SessionHandoff::load"));
    assert!(!engine_sources.contains("SessionHandoff::save"));
    assert!(!PLATFORM_STATE.contains("write_atomic_private"));
    assert!(!PLATFORM_BROWSER.contains("std::fs"));
}

#[test]
fn ashmem_protocol_carries_attempt_scoped_handoff_and_browser_requests() {
    assert!(PLATFORM_IPC.contains("const PROTOCOL_VERSION: u32 = 2"));
    assert!(PLATFORM_IPC.contains("session_handoff: Option<SessionHandoff>"));
    assert!(PLATFORM_IPC.contains("browser_request: Option<BrowserOpenRequest>"));
    assert!(PLATFORM_IPC.contains("browser_request_ack: Option<String>"));
    assert!(PLATFORM_IPC.contains("if scrub_previous_payload"));
    assert!(PLATFORM_IPC.contains("previous_length > content.len()"));
    assert!(PLATFORM_STATE.contains("pub attempt_id: String"));
    assert!(PLATFORM_STATE.contains("pub request_id: String"));
}

#[test]
fn extension_resume_has_no_want_or_disk_credential_fallback() {
    let prepare = section(
        ENGINE_CONNECTION,
        "pub async fn prepare_in_extension",
        "pub async fn attach_tun",
    );
    assert!(prepare.contains("session_handoff_from_ashmem"));
    assert!(!prepare.contains("serde_json::from_str"));
    assert!(!prepare.contains("SessionHandoff::load"));

    let want = section(
        ENGINE,
        "fn sanitized_want_options",
        "fn consume_platform_browser",
    );
    assert!(want.contains("..VpnOptions::default()"));
    assert!(!want.contains("cookie:"));
    assert!(!want.contains("password:"));
}
