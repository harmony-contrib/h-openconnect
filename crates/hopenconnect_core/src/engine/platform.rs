use super::*;

impl SessionEngine {
    pub fn initialize_platform_shared_memory(&self) -> CoreResult<PlatformSharedMemoryFds> {
        {
            let platform = self
                .platform_ipc
                .lock()
                .map_err(|_| CoreError::msg("platform IPC lock poisoned"))?;
            if let Some(platform) = platform.as_ref() {
                let fds = platform.ui_fds().map_err(platform_ipc_error)?;
                return Ok(PlatformSharedMemoryFds {
                    ashmem_fd: fds.ashmem,
                    notification_fd: fds.notification,
                });
            }
        }

        let log_root = {
            let inner = self.lock()?;
            inner.home.clone()
        };
        log_recording::reset_recording(&log_root)?;
        if let Ok(mut logs) = RUNTIME_LOGS.lock() {
            logs.clear();
        }
        let (platform, fds) = PlatformIpc::create_ui().map_err(platform_ipc_error)?;
        {
            let mut slot = self
                .platform_ipc
                .lock()
                .map_err(|_| CoreError::msg("platform IPC lock poisoned"))?;
            *slot = Some(platform);
        }
        let mut inner = self.lock()?;
        self.persist_platform_locked(&mut inner)?;
        Ok(PlatformSharedMemoryFds {
            ashmem_fd: fds.ashmem,
            notification_fd: fds.notification,
        })
    }

    pub fn attach_platform_shared_memory(
        &self,
        ashmem_fd: i32,
        notification_fd: i32,
    ) -> CoreResult<()> {
        let platform =
            PlatformIpc::attach_vpn_raw(ashmem_fd, notification_fd).map_err(platform_ipc_error)?;
        let previous = {
            let mut slot = self
                .platform_ipc
                .lock()
                .map_err(|_| CoreError::msg("platform IPC lock poisoned"))?;
            slot.replace(platform)
        };
        // The extension process may be reused after the UI process restarts.
        // Always bind the latest Want descriptors before checking VPN state.
        drop(previous);
        self.sync_platform_changes()
    }

    pub fn sync_platform_changes(&self) -> CoreResult<()> {
        let mut inner = self.lock()?;
        if self.sync_platform_locked(&mut inner) {
            // The UI acknowledged a one-shot browser request. Publish the
            // Extension lane without that request so a restarted UI cannot
            // consume it again.
            self.persist_platform_locked(&mut inner)?;
        }
        Ok(())
    }

    pub async fn wait_for_platform_change(&self, timeout: Duration) -> CoreResult<bool> {
        let Some(platform) = self.platform_ipc()? else {
            tokio::time::sleep(timeout).await;
            return Ok(false);
        };
        tokio::task::spawn_blocking(move || platform.wait_for_change(timeout))
            .await
            .map_err(|error| CoreError::msg(format!("platform subscription task failed: {error}")))?
            .map_err(platform_ipc_error)
    }

    pub fn queue_platform_browser_open_request(&self, uri: String) -> CoreResult<()> {
        let uri = uri.trim().to_owned();
        if uri.is_empty() {
            return Err(CoreError::msg("external browser URI is empty"));
        }
        let platform = self
            .platform_ipc()?
            .ok_or_else(|| CoreError::msg("platform ashmem is not attached"))?;
        if platform.is_ui() {
            return Err(CoreError::msg(
                "cross-process browser requests must originate from the VPN Extension",
            ));
        }

        let mut inner = self.lock()?;
        if inner.platform_start_attempt_id.is_empty() {
            return Err(CoreError::msg(
                "external browser request has no active VPN start attempt",
            ));
        }
        inner.platform_browser_request_sequence =
            inner.platform_browser_request_sequence.saturating_add(1);
        let requested_at_ms = PlatformVpnState::now_millis();
        inner.platform_browser_request = Some(BrowserOpenRequest {
            request_id: format!(
                "{}-{requested_at_ms}-{}",
                inner.platform_start_attempt_id, inner.platform_browser_request_sequence
            ),
            attempt_id: inner.platform_start_attempt_id.clone(),
            uri,
            requested_at_ms,
        });
        self.persist_platform_locked(&mut inner)
    }

    pub fn take_platform_browser_open_request(&self) -> CoreResult<Option<BrowserOpenRequest>> {
        let platform = match self.platform_ipc()? {
            Some(platform) => platform,
            None => return Ok(None),
        };
        if !platform.is_ui() {
            return Ok(None);
        }
        let request = platform
            .read_remote()
            .map_err(platform_ipc_error)?
            .and_then(|envelope| envelope.browser_request);
        let mut inner = self.lock()?;
        let previous_ack = inner.last_platform_browser_request_id.clone();
        let consumed = consume_platform_browser_request_locked(&mut inner, request);
        if consumed.is_some() {
            if let Err(error) = self.persist_platform_locked(&mut inner) {
                inner.last_platform_browser_request_id = previous_ack;
                return Err(error);
            }
        }
        Ok(consumed)
    }

    pub fn clear_platform_browser_open_request(&self) -> CoreResult<()> {
        let platform = match self.platform_ipc()? {
            Some(platform) => platform,
            None => return Ok(()),
        };
        if platform.is_ui() {
            let remote_request_id = platform
                .read_remote()
                .map_err(platform_ipc_error)?
                .and_then(|envelope| envelope.browser_request)
                .map(|request| request.request_id)
                .unwrap_or_default();
            let mut inner = self.lock()?;
            if inner.last_platform_browser_request_id != remote_request_id {
                inner.last_platform_browser_request_id = remote_request_id;
                self.persist_platform_locked(&mut inner)?;
            }
            return Ok(());
        }

        let mut inner = self.lock()?;
        if inner.platform_browser_request.take().is_some() {
            self.persist_platform_locked(&mut inner)?;
        }
        Ok(())
    }

    pub(super) fn platform_ipc(&self) -> CoreResult<Option<Arc<PlatformIpc>>> {
        self.platform_ipc
            .lock()
            .map(|platform| platform.clone())
            .map_err(|_| CoreError::msg("platform IPC lock poisoned"))
    }

    pub fn begin_platform_vpn_start(&self) -> CoreResult<String> {
        let mut inner = self.lock()?;
        self.sync_platform_locked(&mut inner);
        if inner.platform_start_outcome == PlatformStartOutcome::Pending {
            return Err(CoreError::msg("platform VPN start is already pending"));
        }
        if inner.platform_vpn_running {
            return Err(CoreError::msg("platform VPN is already connected"));
        }

        inner.platform_start_sequence = inner.platform_start_sequence.saturating_add(1);
        let attempt_id = format!(
            "{}-{}",
            PlatformVpnState::now_nanos(),
            inner.platform_start_sequence
        );
        inner.platform_start_attempt_id = attempt_id.clone();
        inner.platform_start_outcome = PlatformStartOutcome::Pending;
        inner.platform_extension_attached = false;
        if let Some(handoff) = inner.platform_session_handoff.as_mut() {
            handoff.attempt_id = attempt_id.clone();
            handoff.updated_at = PlatformVpnState::now_nanos();
        }
        inner.platform_vpn_starting = true;
        inner.platform_vpn_running = false;
        self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Establishing, None);
        self.push_diag_locked(
            &mut inner,
            "info",
            format!("platform VPN start transaction {attempt_id}"),
        );
        self.persist_platform_locked(&mut inner)?;
        Ok(attempt_id)
    }

    /// Bind the extension process to the transaction delivered in its Want.
    pub fn bind_platform_vpn_start(&self, attempt_id: &str) -> CoreResult<()> {
        if attempt_id.is_empty() {
            return Err(CoreError::msg("platform VPN start attempt id is empty"));
        }
        let mut inner = self.lock()?;
        self.sync_platform_locked(&mut inner);
        if inner.platform_start_attempt_id != attempt_id {
            return Err(CoreError::msg(format!(
                "stale platform VPN start attempt {attempt_id}"
            )));
        }
        if matches!(
            inner.platform_start_outcome,
            PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
        ) {
            return Err(CoreError::msg(format!(
                "platform VPN start attempt {attempt_id} is already terminal"
            )));
        }
        if !inner.platform_extension_attached {
            inner.platform_extension_attached = true;
            self.push_diag_locked(
                &mut inner,
                "info",
                format!("platform VPN extension attached to {attempt_id}"),
            );
            self.persist_platform_locked(&mut inner)?;
        }
        Ok(())
    }

    /// Wait briefly for the VPN Extension to accept the matching Want.
    ///
    /// Some HarmonyOS authorization dialogs start the Extension with a new,
    /// parameter-free Want. Callers use this signal to decide whether the
    /// original Want containing the shared-memory descriptors must be sent
    /// again after authorization succeeds.
    pub async fn await_platform_vpn_start_attachment(
        &self,
        attempt_id: &str,
        timeout: Duration,
    ) -> CoreResult<bool> {
        if attempt_id.is_empty() {
            return Err(CoreError::msg("platform VPN start attempt id is empty"));
        }
        let mut receiver = self.platform_start_tx.subscribe();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let event = {
                let mut inner = self.lock()?;
                self.sync_platform_locked(&mut inner);
                self.platform_start_event_locked(&inner)
            };
            if event.attempt_id != attempt_id || event.outcome != PlatformStartOutcome::Pending {
                return Ok(event.attempt_id == attempt_id && event.extension_attached);
            }
            if event.extension_attached {
                return Ok(true);
            }

            tokio::select! {
                changed = receiver.changed() => {
                    changed.map_err(|_| CoreError::msg(
                        "platform VPN start coordinator closed"
                    ))?;
                }
                _ = tokio::time::sleep(Duration::from_millis(25)) => {}
                _ = tokio::time::sleep_until(deadline) => return Ok(false),
            }
        }
    }

    /// Await the authoritative extension terminal state for one transaction.
    ///
    /// This waiter is owned by the caller and ends on Connected, Failed,
    /// Cancelled, replacement, or an IPC error. It does not detach a competing
    /// loop after the system start Promise settles.
    pub async fn await_platform_vpn_start(
        &self,
        attempt_id: &str,
    ) -> CoreResult<PlatformStartOutcome> {
        self.await_platform_vpn_start_with_deadline(attempt_id, PLATFORM_VPN_START_DEADLINE)
            .await
    }

    pub(super) async fn await_platform_vpn_start_with_deadline(
        &self,
        attempt_id: &str,
        timeout: Duration,
    ) -> CoreResult<PlatformStartOutcome> {
        if attempt_id.is_empty() {
            return Err(CoreError::msg("platform VPN start attempt id is empty"));
        }
        let mut receiver = self.platform_start_tx.subscribe();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let event = {
                let mut inner = self.lock()?;
                self.sync_platform_locked(&mut inner);
                self.platform_start_event_locked(&inner)
            };
            if event.attempt_id != attempt_id {
                return Err(CoreError::msg(format!(
                    "platform VPN start attempt {attempt_id} was superseded"
                )));
            }
            match event.outcome {
                PlatformStartOutcome::Connected => {
                    self.clear_platform_session_handoff(attempt_id)?;
                    return Ok(PlatformStartOutcome::Connected);
                }
                PlatformStartOutcome::Failed => {
                    let error = event
                        .error
                        .unwrap_or_else(|| "VPN extension failed to start".to_owned());
                    self.clear_platform_session_handoff(attempt_id)?;
                    return Err(CoreError::msg(error));
                }
                PlatformStartOutcome::Cancelled => {
                    self.clear_platform_session_handoff(attempt_id)?;
                    return Err(CoreError::msg("VPN extension start was cancelled"));
                }
                PlatformStartOutcome::Idle => {
                    self.clear_platform_session_handoff(attempt_id)?;
                    return Err(CoreError::msg(format!(
                        "platform VPN start attempt {attempt_id} is not active"
                    )));
                }
                PlatformStartOutcome::Pending => {}
            }

            tokio::select! {
                changed = receiver.changed() => {
                    changed.map_err(|_| CoreError::msg("platform VPN start coordinator closed"))?;
                }
                changed = self.wait_for_platform_change(Duration::from_secs(1)) => {
                    changed?;
                }
                _ = tokio::time::sleep_until(deadline) => {
                    self.fail_platform_vpn_start(
                        attempt_id,
                        "VPN extension did not reach a terminal state before the startup deadline".to_owned(),
                    )?;
                }
            }
        }
    }

    pub(super) fn clear_platform_session_handoff(&self, attempt_id: &str) -> CoreResult<()> {
        let mut inner = self.lock()?;
        let matches_attempt = inner
            .platform_session_handoff
            .as_ref()
            .is_some_and(|handoff| handoff.attempt_id == attempt_id);
        if matches_attempt {
            inner.platform_session_handoff = None;
            self.persist_platform_locked(&mut inner)?;
        }
        Ok(())
    }

    /// Fail a matching request only when the VPN Extension has not accepted it.
    ///
    /// The UI uses this for an explicit system dispatch rejection. Once the
    /// Extension has accepted the matching Want, only its ashmem terminal (or
    /// the transaction deadline) may decide the outcome.
    pub fn fail_unattached_platform_vpn_start(
        &self,
        attempt_id: &str,
        error: String,
    ) -> CoreResult<bool> {
        self.fail_platform_vpn_start_if(attempt_id, error, true)
    }

    /// Convert a system dispatch rejection into this attempt's terminal state.
    /// A late rejection for an older attempt is ignored by returning `false`.
    pub fn fail_platform_vpn_start(&self, attempt_id: &str, error: String) -> CoreResult<bool> {
        self.fail_platform_vpn_start_if(attempt_id, error, false)
    }

    fn fail_platform_vpn_start_if(
        &self,
        attempt_id: &str,
        error: String,
        require_unattached: bool,
    ) -> CoreResult<bool> {
        #[cfg(feature = "native-anyconnect")]
        let stop_native = {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            if !self.platform_start_is_pending_locked(&inner, attempt_id)
                || (require_unattached && inner.platform_extension_attached)
            {
                return Ok(false);
            }
            let stop = self.apply_platform_vpn_failed_locked(&mut inner, error)?;
            inner.platform_start_outcome = PlatformStartOutcome::Failed;
            self.persist_platform_locked(&mut inner)?;
            stop
        };
        #[cfg(not(feature = "native-anyconnect"))]
        {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            if !self.platform_start_is_pending_locked(&inner, attempt_id)
                || (require_unattached && inner.platform_extension_attached)
            {
                return Ok(false);
            }
            self.apply_platform_vpn_failed_locked(&mut inner, error)?;
            inner.platform_start_outcome = PlatformStartOutcome::Failed;
            self.persist_platform_locked(&mut inner)?;
        }
        #[cfg(feature = "native-anyconnect")]
        if let Some(session) = stop_native {
            session.cancel();
            let _ = session.join(Duration::from_secs(2));
        }
        Ok(true)
    }

    /// Cancel only the matching pending transaction.
    pub fn cancel_platform_vpn_start(&self, attempt_id: &str) -> CoreResult<bool> {
        let mut inner = self.lock()?;
        self.sync_platform_locked(&mut inner);
        if !self.platform_start_is_pending_locked(&inner, attempt_id) {
            return Ok(false);
        }
        inner.platform_vpn_starting = false;
        inner.platform_vpn_running = false;
        inner.platform_start_outcome = PlatformStartOutcome::Cancelled;
        inner.platform_session_handoff = None;
        inner.platform_browser_request = None;
        #[cfg(feature = "native-anyconnect")]
        {
            inner.pending_native = None;
        }
        self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Disconnected, None);
        self.push_diag_locked(
            &mut inner,
            "info",
            format!("platform VPN start transaction {attempt_id} cancelled"),
        );
        self.persist_platform_locked(&mut inner)?;
        Ok(true)
    }

    pub fn set_platform_vpn_starting(&self, starting: bool) -> CoreResult<()> {
        let mut inner = self.lock()?;
        self.sync_platform_locked(&mut inner);
        // Extension already marked running — do not regress to "starting".
        if starting && inner.platform_vpn_running {
            return Ok(());
        }
        inner.platform_vpn_starting = starting;
        if starting {
            inner.platform_vpn_running = false;
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Establishing, None);
            self.push_diag_locked(&mut inner, "info", "platform VPN extension starting");
        }
        self.persist_platform_locked(&mut inner)?;
        Ok(())
    }

    pub fn set_platform_vpn_running(&self, running: bool) -> CoreResult<()> {
        #[cfg(feature = "native-anyconnect")]
        let stop_native = {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            let stop = self.apply_platform_vpn_running_locked(&mut inner, running)?;
            self.persist_platform_locked(&mut inner)?;
            stop
        };
        #[cfg(not(feature = "native-anyconnect"))]
        {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            self.apply_platform_vpn_running_locked(&mut inner, running)?;
            self.persist_platform_locked(&mut inner)?;
        }
        #[cfg(feature = "native-anyconnect")]
        if let Some(session) = stop_native {
            session.cancel();
            let _ = session.join(Duration::from_secs(5));
        }
        Ok(())
    }

    pub fn set_platform_vpn_failed(&self, error: String) -> CoreResult<()> {
        #[cfg(feature = "native-anyconnect")]
        let stop_native = {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            let stop = self.apply_platform_vpn_failed_locked(&mut inner, error)?;
            self.persist_platform_locked(&mut inner)?;
            stop
        };
        #[cfg(not(feature = "native-anyconnect"))]
        {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            self.apply_platform_vpn_failed_locked(&mut inner, error)?;
            self.persist_platform_locked(&mut inner)?;
        }
        #[cfg(feature = "native-anyconnect")]
        if let Some(session) = stop_native {
            session.cancel();
            let _ = session.join(Duration::from_secs(2));
        }
        Ok(())
    }

    #[cfg(feature = "native-anyconnect")]
    fn apply_platform_vpn_running_locked(
        &self,
        inner: &mut Inner,
        running: bool,
    ) -> CoreResult<Option<RunningNativeSession>> {
        inner.platform_vpn_running = running;
        inner.platform_vpn_starting = false;
        inner.platform_browser_request = None;
        if running {
            if inner.platform_start_outcome == PlatformStartOutcome::Pending {
                inner.platform_start_outcome = PlatformStartOutcome::Connected;
            }
            if !inner.snapshot.lifecycle.is_active() {
                self.set_lifecycle_locked(inner, ConnectionLifecycle::Connected, None);
                inner.connected_at = Some(Instant::now());
            }
            self.push_diag_locked(inner, "info", "platform VPN TUN is up");
            return Ok(None);
        }
        if inner.platform_start_outcome == PlatformStartOutcome::Pending {
            inner.platform_start_outcome = PlatformStartOutcome::Cancelled;
        }
        let stop = if matches!(
            inner.snapshot.lifecycle,
            ConnectionLifecycle::Connected
                | ConnectionLifecycle::Establishing
                | ConnectionLifecycle::Disconnecting
        ) {
            let session = inner.running_native.take();
            inner.pending_native = None;
            self.set_lifecycle_locked(inner, ConnectionLifecycle::Disconnected, None);
            inner.connected_at = None;
            inner.snapshot.stats = SessionStats::default();
            session
        } else {
            None
        };
        Ok(stop)
    }

    #[cfg(not(feature = "native-anyconnect"))]
    fn apply_platform_vpn_running_locked(
        &self,
        inner: &mut Inner,
        running: bool,
    ) -> CoreResult<()> {
        inner.platform_vpn_running = running;
        inner.platform_vpn_starting = false;
        inner.platform_browser_request = None;
        if running {
            if inner.platform_start_outcome == PlatformStartOutcome::Pending {
                inner.platform_start_outcome = PlatformStartOutcome::Connected;
            }
            if !inner.snapshot.lifecycle.is_active() {
                self.set_lifecycle_locked(inner, ConnectionLifecycle::Connected, None);
                inner.connected_at = Some(Instant::now());
            }
            self.push_diag_locked(inner, "info", "platform VPN TUN is up");
        } else {
            if inner.platform_start_outcome == PlatformStartOutcome::Pending {
                inner.platform_start_outcome = PlatformStartOutcome::Cancelled;
            }
        }
        if !running
            && matches!(
                inner.snapshot.lifecycle,
                ConnectionLifecycle::Connected
                    | ConnectionLifecycle::Establishing
                    | ConnectionLifecycle::Disconnecting
            )
        {
            self.set_lifecycle_locked(inner, ConnectionLifecycle::Disconnected, None);
            inner.connected_at = None;
            inner.snapshot.stats = SessionStats::default();
        }
        Ok(())
    }

    #[cfg(feature = "native-anyconnect")]
    fn apply_platform_vpn_failed_locked(
        &self,
        inner: &mut Inner,
        error: String,
    ) -> CoreResult<Option<RunningNativeSession>> {
        inner.platform_vpn_starting = false;
        inner.platform_vpn_running = false;
        inner.platform_session_handoff = None;
        inner.platform_browser_request = None;
        if inner.platform_start_outcome == PlatformStartOutcome::Pending {
            inner.platform_start_outcome = PlatformStartOutcome::Failed;
        }
        let session = inner.running_native.take();
        inner.pending_native = None;
        self.set_lifecycle_locked(inner, ConnectionLifecycle::Failed, Some(error.clone()));
        self.push_diag_locked(inner, "error", error.clone());
        Ok(session)
    }

    #[cfg(not(feature = "native-anyconnect"))]
    fn apply_platform_vpn_failed_locked(&self, inner: &mut Inner, error: String) -> CoreResult<()> {
        inner.platform_vpn_starting = false;
        inner.platform_vpn_running = false;
        inner.platform_session_handoff = None;
        inner.platform_browser_request = None;
        if inner.platform_start_outcome == PlatformStartOutcome::Pending {
            inner.platform_start_outcome = PlatformStartOutcome::Failed;
        }
        self.set_lifecycle_locked(inner, ConnectionLifecycle::Failed, Some(error.clone()));
        self.push_diag_locked(inner, "error", error.clone());
        Ok(())
    }

    pub fn expire_platform_vpn_start(&self) -> CoreResult<bool> {
        let mut inner = self.lock()?;
        // VPN extension may already have published running=true to ashmem.
        self.sync_platform_locked(&mut inner);
        if !inner.platform_vpn_starting || inner.platform_vpn_running {
            return Ok(false);
        }
        #[cfg(feature = "native-anyconnect")]
        {
            if inner.running_native.is_some() {
                return Ok(false);
            }
        }
        inner.platform_vpn_starting = false;
        inner.platform_session_handoff = None;
        inner.platform_browser_request = None;
        if inner.platform_start_outcome == PlatformStartOutcome::Pending {
            inner.platform_start_outcome = PlatformStartOutcome::Failed;
        }
        #[cfg(feature = "native-anyconnect")]
        {
            inner.pending_native = None;
        }
        self.set_lifecycle_locked(
            &mut inner,
            ConnectionLifecycle::Failed,
            Some("VPN extension startup timed out".to_owned()),
        );
        self.persist_platform_locked(&mut inner)?;
        Ok(true)
    }

    /// Read the newest sibling-process frame from the opposite ashmem lane.
    pub(super) fn sync_platform_locked(&self, inner: &mut Inner) -> bool {
        let Some(platform) = self.platform_ipc().ok().flatten() else {
            return false;
        };
        let envelope = match platform.read_remote() {
            Ok(Some(envelope)) => envelope,
            Ok(None) => return false,
            Err(error) => {
                self.push_diag_locked(
                    inner,
                    "warn",
                    format!("read platform shared memory failed: {error}"),
                );
                return false;
            }
        };
        let is_ui = platform.is_ui();
        let browser_request_acknowledged = !is_ui
            && acknowledge_platform_browser_request_locked(
                inner,
                envelope.browser_request_ack.as_deref(),
            );
        let Some(remote) = envelope
            .state
            .filter(|state| state.updated_at > inner.platform_vpn_state_updated_at)
        else {
            return browser_request_acknowledged;
        };
        let remote_attempt_matches = !remote.start_attempt_id.is_empty()
            && remote.start_attempt_id == inner.platform_start_attempt_id;

        // The UI owns transaction creation and only accepts replies for its
        // current id. The extension adopts a new id from the UI lane when the
        // corresponding Want is attached. Neither side may regress a terminal
        // outcome back to Pending.
        if is_ui && !remote_attempt_matches {
            return browser_request_acknowledged;
        }
        if !is_ui
            && !remote.start_attempt_id.is_empty()
            && remote.start_attempt_id != inner.platform_start_attempt_id
        {
            inner.platform_start_attempt_id = remote.start_attempt_id.clone();
            inner.platform_start_outcome = remote.start_outcome;
            inner.platform_extension_attached = remote.extension_attached;
        } else if remote_attempt_matches
            && inner.platform_start_outcome == PlatformStartOutcome::Pending
            && matches!(
                remote.start_outcome,
                PlatformStartOutcome::Connected
                    | PlatformStartOutcome::Failed
                    | PlatformStartOutcome::Cancelled
            )
        {
            inner.platform_start_outcome = remote.start_outcome;
        }
        if remote_attempt_matches && remote.extension_attached {
            inner.platform_extension_attached = true;
        }

        #[cfg(feature = "native-anyconnect")]
        let local_mainloop = inner.running_native.is_some();
        #[cfg(not(feature = "native-anyconnect"))]
        let local_mainloop = false;

        let was_running = inner.platform_vpn_running;
        let was_starting = inner.platform_vpn_starting;
        let running = local_mainloop || remote.running;
        let starting = !running && remote.starting;
        inner.platform_vpn_starting = starting;
        inner.platform_vpn_running = running;
        if running && inner.platform_start_outcome == PlatformStartOutcome::Pending {
            inner.platform_start_outcome = PlatformStartOutcome::Connected;
        } else if !running
            && inner.platform_start_outcome == PlatformStartOutcome::Pending
            && (matches!(remote.lifecycle, ConnectionLifecycle::Failed)
                || remote.last_error.is_some())
        {
            inner.platform_start_outcome = PlatformStartOutcome::Failed;
        }
        inner.platform_vpn_state_updated_at = remote.updated_at;
        inner.platform_diagnostics = remote.diagnostics.clone();

        if running {
            if !remote.assigned_ip.is_empty() {
                inner.snapshot.stats.assigned_ip = remote.assigned_ip.clone();
            }
            if !remote.gateway.is_empty() {
                inner.snapshot.stats.gateway = remote.gateway.clone();
            }
            if remote.mtu > 0 {
                inner.snapshot.stats.mtu = remote.mtu;
            }
            if remote.network.address.is_some() || !remote.network.dns.is_empty() {
                inner.snapshot.network = remote.network.clone();
            }
            // Prefer richer stats from the extension (bytes counters).
            if remote.stats.bytes_sent > inner.snapshot.stats.bytes_sent
                || remote.stats.bytes_received > inner.snapshot.stats.bytes_received
            {
                let mut stats = remote.stats.clone();
                if stats.assigned_ip.is_empty() {
                    stats.assigned_ip = inner.snapshot.stats.assigned_ip.clone();
                }
                if stats.gateway.is_empty() {
                    stats.gateway = inner.snapshot.stats.gateway.clone();
                }
                if stats.mtu == 0 {
                    stats.mtu = inner.snapshot.stats.mtu;
                }
                inner.snapshot.stats = stats;
            }
            if !inner.snapshot.lifecycle.is_active()
                && !matches!(inner.snapshot.lifecycle, ConnectionLifecycle::Disconnecting)
            {
                self.set_lifecycle_locked(inner, ConnectionLifecycle::Connected, None);
                if inner.connected_at.is_none() {
                    inner.connected_at = Some(Instant::now());
                }
            }
        } else if starting {
            if !inner.snapshot.lifecycle.is_active()
                && !matches!(inner.snapshot.lifecycle, ConnectionLifecycle::Failed)
            {
                self.set_lifecycle_locked(inner, ConnectionLifecycle::Establishing, None);
            }
        } else if matches!(remote.lifecycle, ConnectionLifecycle::Failed)
            || remote.last_error.is_some()
        {
            if !matches!(
                inner.snapshot.lifecycle,
                ConnectionLifecycle::Failed | ConnectionLifecycle::Disconnected
            ) {
                self.set_lifecycle_locked(
                    inner,
                    ConnectionLifecycle::Failed,
                    remote.last_error.clone(),
                );
            }
        } else if (was_running || was_starting)
            && matches!(
                inner.snapshot.lifecycle,
                ConnectionLifecycle::Connected | ConnectionLifecycle::Establishing
            )
        {
            self.set_lifecycle_locked(inner, ConnectionLifecycle::Disconnected, None);
            inner.connected_at = None;
        }
        self.notify_platform_start_locked(inner);
        browser_request_acknowledged
    }

    pub(super) fn persist_platform_locked(&self, inner: &mut Inner) -> CoreResult<()> {
        // Device SystemTime can be coarser than nanoseconds. Always advance the
        // revision so a terminal state cannot be discarded as a duplicate.
        inner.platform_vpn_state_updated_at = PlatformVpnState::now_nanos()
            .max(inner.platform_vpn_state_updated_at.saturating_add(1));
        let Some(platform) = self.platform_ipc()? else {
            self.notify_platform_start_locked(inner);
            return Ok(());
        };
        let state = PlatformVpnState {
            start_attempt_id: inner.platform_start_attempt_id.clone(),
            start_outcome: inner.platform_start_outcome,
            extension_attached: inner.platform_extension_attached,
            starting: inner.platform_vpn_starting,
            running: inner.platform_vpn_running || {
                #[cfg(feature = "native-anyconnect")]
                {
                    inner.running_native.is_some()
                }
                #[cfg(not(feature = "native-anyconnect"))]
                {
                    false
                }
            },
            lifecycle: inner.snapshot.lifecycle,
            last_error: inner.snapshot.last_error.clone(),
            assigned_ip: inner.snapshot.stats.assigned_ip.clone(),
            gateway: inner.snapshot.stats.gateway.clone(),
            mtu: inner.snapshot.stats.mtu,
            network: inner.snapshot.network.clone(),
            stats: inner.snapshot.stats.clone(),
            diagnostics: merged_logs(&inner.logs),
            updated_at: inner.platform_vpn_state_updated_at,
        };
        self.notify_platform_start_locked(inner);
        let browser_request_ack =
            if platform.is_ui() && !inner.last_platform_browser_request_id.is_empty() {
                Some(inner.last_platform_browser_request_id.clone())
            } else {
                None
            };
        platform
            .publish_snapshot(
                state,
                inner.platform_session_handoff.clone(),
                inner.platform_browser_request.clone(),
                browser_request_ack,
            )
            .map_err(platform_ipc_error)
    }

    fn platform_start_is_pending_locked(&self, inner: &Inner, attempt_id: &str) -> bool {
        !attempt_id.is_empty()
            && inner.platform_start_attempt_id == attempt_id
            && inner.platform_start_outcome == PlatformStartOutcome::Pending
    }

    fn platform_start_event_locked(&self, inner: &Inner) -> PlatformStartEvent {
        PlatformStartEvent {
            attempt_id: inner.platform_start_attempt_id.clone(),
            outcome: inner.platform_start_outcome,
            extension_attached: inner.platform_extension_attached,
            error: inner.snapshot.last_error.clone(),
        }
    }

    fn notify_platform_start_locked(&self, inner: &Inner) {
        let event = self.platform_start_event_locked(inner);
        let changed = {
            let current = self.platform_start_tx.borrow();
            *current != event
        };
        if changed {
            self.platform_start_tx.send_replace(event);
        }
    }
}
