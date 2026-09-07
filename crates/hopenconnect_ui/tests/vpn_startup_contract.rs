const ENTRY_ABILITY: &str =
    include_str!("../../../entry/src/main/ets/entryability/EntryAbility.ets");
const VPN_PLUGIN: &str = include_str!("../../../entry/src/main/ets/plugins/VpnPlugin.ets");
const VPN_ABILITY: &str =
    include_str!("../../../entry/src/main/ets/vpnability/HOpenConnectVpnExtensionAbility.ets");
const VPN_CONFIG: &str = include_str!("../../../entry/src/main/ets/vpnability/VpnConfig.ets");
const NAPI_TYPES: &str =
    include_str!("../../../entry/src/main/cpp/types/libhopenconnect_ui/Index.d.ts");
const UI_STATE: &str = include_str!("../src/state.rs");

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
        "let attemptId: string",
    );

    assert!(bootstrap.contains("authorization bootstrap"));
    assert!(bootstrap.contains("waiting for rebound request"));
    assert!(!bootstrap.contains("setPlatformVpnFailed"));
}

#[test]
fn tick_observes_native_terminal_state_before_auto_reconnect_edge() {
    let tick = section(
        UI_STATE,
        "Action::TickSession =>",
        "Action::SetChallengeField",
    );
    let previous = tick
        .find("let prev = state.last_lifecycle")
        .expect("old state");
    let engine_tick = tick.find("shared_engine().tick()").expect("engine tick");
    let sync = tick.find("state.sync_engine()").expect("state sync");
    let edge = tick
        .find("prev.should_auto_reconnect_to")
        .expect("reconnect edge");
    let remember = tick
        .find("state.last_lifecycle = now")
        .expect("remember terminal state");

    assert!(previous < engine_tick);
    assert!(engine_tick < sync);
    assert!(sync < edge);
    assert!(edge < remember);
}

#[test]
fn extension_rebinds_event_waiter_and_has_bounded_start_operations() {
    assert!(NAPI_TYPES.contains("extensionTick(): string"));
    let handle = section(
        VPN_ABILITY,
        "private async handleRequest",
        "private isCurrentRequest",
    );
    let stop_waiter = handle
        .find("this.stopPlatformSubscription()")
        .expect("stop old subscription");
    let attach = handle
        .find("attachPlatformSharedMemory")
        .expect("attach new shared memory");
    let restart_waiter = handle
        .find("this.startPlatformSubscription()")
        .expect("restart subscription");
    assert!(stop_waiter < attach);
    assert!(attach < restart_waiter);

    let start = section(
        VPN_ABILITY,
        "private async performStart",
        "private extensionTick",
    );
    assert!(start.contains("withTimeout("));
    assert!(start.contains("PREPARE_TIMEOUT_MS"));
    assert!(start.contains("PLATFORM_OPERATION_TIMEOUT_MS"));
    assert!(VPN_ABILITY.contains("setInterval"));
    assert!(VPN_ABILITY.contains("reconcileExtensionTerminal"));
}

#[test]
fn stale_wants_and_terminal_cleanup_are_attempt_scoped_before_mutation() {
    assert!(NAPI_TYPES.contains("validatePlatformVpnStartRequest("));
    assert!(NAPI_TYPES.contains("stopVpn(attemptId: string): Promise<boolean>"));

    let start = section(
        VPN_ABILITY,
        "private startFromWant",
        "private async handleRequest",
    );
    let validate = start.find("this.isLatestWant(").expect("Want validation");
    let generation = start
        .find("++this.requestGeneration")
        .expect("request generation");
    assert!(validate < generation);

    let handle = section(
        VPN_ABILITY,
        "private async handleRequest",
        "private isCurrentRequest",
    );
    let validate = handle
        .find("this.isLatestWant(")
        .expect("queued validation");
    let destroy = handle.find("await this.destroyVpn()").expect("old cleanup");
    let attach = handle
        .find("attachPlatformSharedMemory")
        .expect("IPC replacement");
    assert!(validate < destroy);
    assert!(validate < attach);

    let reconcile = section(
        VPN_ABILITY,
        "private reconcileExtensionTerminal",
        "private beginVpnCleanup",
    );
    let stop_subscription = reconcile
        .find("this.stopPlatformSubscription()")
        .expect("owner freeze");
    let enqueue_cleanup = reconcile
        .find("this.requestChain =")
        .expect("serialized cleanup");
    assert!(stop_subscription < enqueue_cleanup);

    let cleanup = section(
        VPN_ABILITY,
        "private beginVpnCleanup",
        "private async waitForPendingCleanup",
    );
    assert!(cleanup.contains("hopenconnectUi.stopVpn(attemptId)"));
    assert!(!cleanup.contains("hopenconnectUi.stopVpn()"));
}

#[test]
fn first_authorization_want_unwraps_nested_parameters_and_descriptors() {
    assert!(VPN_CONFIG.contains("const HOPEN_SYSTEM_PARAMETERS_KEY = 'myParams'"));
    assert!(VPN_CONFIG.contains("function readVpnParameters"));
    assert!(VPN_CONFIG.contains("readFileDescriptorParameter"));
    assert!(VPN_CONFIG.contains("readPlatformStartAttemptId"));
}
