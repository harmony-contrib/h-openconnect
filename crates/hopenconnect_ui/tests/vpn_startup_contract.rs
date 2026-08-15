const ENTRY_ABILITY: &str =
    include_str!("../../../entry/src/main/ets/entryability/EntryAbility.ets");
const VPN_PLUGIN: &str = include_str!("../../../entry/src/main/ets/plugins/VpnPlugin.ets");
const VPN_ABILITY: &str =
    include_str!("../../../entry/src/main/ets/vpnability/HOpenConnectVpnExtensionAbility.ets");
const VPN_CONFIG: &str = include_str!("../../../entry/src/main/ets/vpnability/VpnConfig.ets");
const NAPI_TYPES: &str =
    include_str!("../../../entry/src/main/cpp/types/libhopenconnect_ui/Index.d.ts");

fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start = source.find(start).expect("section start");
    let tail = &source[start..];
    let end = tail.find(end).expect("section end");
    &tail[..end]
}

#[test]
fn first_authorization_start_is_coordinated_by_the_extension_terminal_state() {
    assert!(NAPI_TYPES.contains("beginPlatformVpnStart(): string"));
    assert!(NAPI_TYPES.contains("bindPlatformVpnStart(attemptId: string): void"));
    assert!(NAPI_TYPES.contains("awaitPlatformVpnStartAttachment("));
    assert!(NAPI_TYPES.contains("awaitPlatformVpnStart(attemptId: string): Promise<string>"));
    assert!(NAPI_TYPES.contains("failUnattachedPlatformVpnStart("));

    let request = section(
        VPN_PLUGIN,
        "private dispatchVpnStart",
        "private async requestStopVpnWithContext",
    );
    assert!(request.contains("beginPlatformVpnStart()"));
    assert!(request.contains("awaitPlatformVpnStart(attemptId)"));
    assert!(request.contains("failUnattachedPlatformVpnStart(attemptId, message)"));
    assert!(request.contains("buildVpnWant(optionsJson, this.platformSharedMemory, attemptId)"));
    assert!(request.contains("awaitPlatformVpnStartAttachment"));
    assert!(request.contains("redispatching attempt"));
    assert!(!request.contains("Promise.race"));
    assert!(!request.contains("15000"));

    let extension = section(
        VPN_ABILITY,
        "private startFromWant",
        "private startPlatformSubscription",
    );
    let attach = extension
        .find("attachPlatformSharedMemory")
        .expect("ashmem attachment");
    let bind = extension
        .find("bindPlatformVpnStart")
        .expect("attempt binding");
    let running = extension
        .find("setPlatformVpnRunning")
        .expect("terminal state");
    assert!(attach < bind);
    assert!(bind < running);
}

#[test]
fn descriptor_free_authorization_bootstrap_waits_for_the_rebound_want() {
    let start = section(
        VPN_ABILITY,
        "private startFromWant",
        "private startPlatformSubscription",
    );
    let bootstrap = section(
        start,
        "const sharedMemory = readPlatformSharedMemoryFds(want)",
        "try {\n      hopenconnectUi.attachPlatformSharedMemory",
    );

    assert!(bootstrap.contains("authorization bootstrap"));
    assert!(bootstrap.contains("waiting for rebound request"));
    assert!(!bootstrap.contains("setPlatformVpnFailed"));
}

#[test]
fn first_authorization_want_unwraps_nested_parameters_and_descriptors() {
    assert!(VPN_CONFIG.contains("const HOPEN_SYSTEM_PARAMETERS_KEY = 'myParams'"));
    assert!(VPN_CONFIG.contains("function readVpnParameters"));
    assert!(VPN_CONFIG.contains("readFileDescriptorParameter"));
    assert!(VPN_CONFIG.contains("readPlatformStartAttemptId"));
}
