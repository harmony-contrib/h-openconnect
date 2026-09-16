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
        // The Extension process may be reused after the UI process restarts.
        // Cancel only the waiter bound to the replaced session before dropping
        // it, then let the subscription loop re-enter on the latest binding.
        if let Some(previous) = previous {
            previous.cancel_event_waits();
        }
        {
            let mut inner = self.lock()?;
            inner.platform_remote_state_updated_at = 0;
            inner.platform_remote_state_seen_at = None;
            inner.platform_remote_stale_since = None;
        }
        // Do not consume/adopt the UI transaction here. A reused Extension
        // may still be cleaning up its previous owner when a new Want arrives.
        // Only bind_platform_vpn_start, with the explicit attempt id carried
        // by that Want, is allowed to transfer ownership.
        Ok(())
    }

    /// Validate a delivered Want against its own shared-memory UI lane without
    /// replacing this process's current IPC binding or session owner.
    pub fn validate_platform_vpn_start_request(
        &self,
        ashmem_fd: i32,
        notification_fd: i32,
        attempt_id: &str,
    ) -> CoreResult<()> {
        self.validate_platform_owner_journal_for_want(attempt_id)?;
        let platform =
            PlatformIpc::attach_vpn_raw(ashmem_fd, notification_fd).map_err(platform_ipc_error)?;
        let envelope = platform
            .read_remote()
            .map_err(platform_ipc_error)?
            .ok_or_else(|| CoreError::msg("platform VPN start has no UI state"))?;
        validate_platform_start_envelope(&envelope, attempt_id)
    }

    fn validate_platform_owner_journal_for_want(&self, attempt_id: &str) -> CoreResult<()> {
        let (journal_path, issuer_lease_path) = {
            let inner = self.lock()?;
            (
                platform_owner_journal_path(&inner),
                platform_owner_lease_path(&inner, PlatformVpnOwnerLeaseRole::Issuer),
            )
        };
        let journal = match crate::platform_owner::read(&journal_path)? {
            JournalRead::Missing => {
                return Err(CoreError::msg(format!(
                    "platform VPN owner journal is missing for delivered attempt {attempt_id}"
                )))
            }
            JournalRead::Present(journal) if journal.attempt_id == attempt_id => journal,
            JournalRead::Present(journal) => {
                return Err(CoreError::msg(format!(
                    "stale platform VPN start attempt {attempt_id}; owner journal belongs to {}",
                    journal.attempt_id
                )))
            }
        };
        match journal.phase {
            PlatformVpnOwnerPhase::Pending => {
                let expected = platform_owner_lease_record(
                    attempt_id,
                    journal.issuer,
                    PlatformVpnOwnerLeaseRole::Issuer,
                );
                match crate::platform_owner::observe_owner_lease_exact(
                    &issuer_lease_path,
                    &expected,
                )? {
                    PlatformVpnOwnerLeaseObservation::HeldExact => {}
                    PlatformVpnOwnerLeaseObservation::Released => {
                        return Err(CoreError::msg(format!(
                            "platform VPN start issuer lease for delivered attempt {attempt_id} was released"
                        )))
                    }
                    PlatformVpnOwnerLeaseObservation::HeldOther => {
                        return Err(CoreError::msg(format!(
                            "cannot verify the exact platform VPN start issuer lease for delivered attempt {attempt_id}"
                        )))
                    }
                }
            }
            PlatformVpnOwnerPhase::Attached => {}
            PlatformVpnOwnerPhase::Stopping => {
                return Err(CoreError::msg(format!(
                    "platform VPN start attempt {attempt_id} was fenced by a stop intent"
                )))
            }
        }
        Ok(())
    }

    pub fn acknowledge_terminal_platform_vpn_start_delivery(
        &self,
        ashmem_fd: i32,
        notification_fd: i32,
        attempt_id: &str,
    ) -> CoreResult<bool> {
        if attempt_id.is_empty() {
            return Ok(false);
        }
        let platform =
            PlatformIpc::attach_vpn_raw(ashmem_fd, notification_fd).map_err(platform_ipc_error)?;
        let envelope = platform
            .read_remote()
            .map_err(platform_ipc_error)?
            .ok_or_else(|| CoreError::msg("platform VPN start has no UI state"))?;
        let Some(mut state) = envelope.state else {
            return Ok(false);
        };
        if !acknowledge_terminal_delivery_state(&mut state, attempt_id) {
            return Ok(false);
        }
        platform.publish_state(state).map_err(platform_ipc_error)?;
        Ok(true)
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

    /// Block until the peer process publishes a platform frame.
    ///
    /// Fully event driven: the wait parks on the session notification socket
    /// together with a process-local cancellation socket. It resolves when a
    /// frame arrives (`Ok(true)`) or when [`Self::cancel_platform_change_wait`]
    /// is invoked (`Ok(false)`); it never polls.
    pub async fn wait_for_platform_change_event(&self) -> CoreResult<bool> {
        let Some(platform) = self.platform_ipc()? else {
            return Ok(false);
        };
        tokio::task::spawn_blocking(move || platform.wait_for_change_event_cancellable())
            .await
            .map_err(|error| CoreError::msg(format!("platform subscription task failed: {error}")))?
            .map_err(platform_ipc_error)
    }

    /// Wake the in-process waiter parked in [`Self::wait_for_platform_change_event`].
    pub fn cancel_platform_change_wait(&self) {
        if let Ok(Some(platform)) = self.platform_ipc() {
            platform.cancel_event_waits();
        }
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

    /// Issue a process-wide ordering token for one VPN Start/Stop intent.
    pub fn advance_platform_vpn_intent(&self) -> CoreResult<u64> {
        let mut inner = self.lock()?;
        inner.platform_vpn_intent_epoch = inner
            .platform_vpn_intent_epoch
            .checked_add(1)
            .ok_or_else(|| CoreError::msg("platform VPN intent epoch exhausted"))?;
        Ok(inner.platform_vpn_intent_epoch)
    }

    pub fn is_platform_vpn_intent_current(&self, intent_epoch: u64) -> CoreResult<bool> {
        let inner = self.lock()?;
        Ok(intent_epoch > 0 && inner.platform_vpn_intent_epoch == intent_epoch)
    }

    pub fn is_platform_vpn_stop_current(
        &self,
        intent_epoch: u64,
        attempt_id: &str,
    ) -> CoreResult<bool> {
        let inner = self.lock()?;
        let exact_fence = inner.platform_os_stop_epoch == intent_epoch
            && inner.platform_os_stop_attempt_id == attempt_id;
        Ok(intent_epoch > 0
            && inner.platform_vpn_intent_epoch == intent_epoch
            && exact_fence
            && !inner.platform_os_stop_in_flight)
    }

    pub fn begin_platform_vpn_os_stop(
        &self,
        intent_epoch: u64,
        attempt_id: &str,
    ) -> CoreResult<bool> {
        let mut inner = self.lock()?;
        if intent_epoch == 0
            || inner.platform_vpn_intent_epoch != intent_epoch
            || inner.platform_os_stop_in_flight
        {
            return Ok(false);
        }
        if inner.platform_os_stop_epoch == 0 {
            if !attempt_id.is_empty() {
                return Ok(false);
            }
            inner.platform_os_stop_epoch = intent_epoch;
            inner.platform_os_stop_attempt_id.clear();
        } else if inner.platform_os_stop_epoch != intent_epoch
            || inner.platform_os_stop_attempt_id != attempt_id
        {
            return Ok(false);
        }
        inner.platform_os_stop_in_flight = true;
        Ok(true)
    }

    pub fn complete_platform_vpn_os_stop(
        &self,
        intent_epoch: u64,
        attempt_id: &str,
    ) -> CoreResult<bool> {
        let mut inner = self.lock()?;
        if inner.platform_os_stop_epoch != intent_epoch
            || inner.platform_os_stop_attempt_id != attempt_id
            || !inner.platform_os_stop_in_flight
        {
            return Ok(false);
        }
        inner.platform_os_stop_epoch = 0;
        inner.platform_os_stop_attempt_id.clear();
        inner.platform_os_stop_in_flight = false;
        Ok(true)
    }

    pub fn fail_platform_vpn_os_stop(
        &self,
        intent_epoch: u64,
        attempt_id: &str,
    ) -> CoreResult<bool> {
        let mut inner = self.lock()?;
        if inner.platform_os_stop_epoch != intent_epoch
            || inner.platform_os_stop_attempt_id != attempt_id
            || !inner.platform_os_stop_in_flight
        {
            return Ok(false);
        }
        inner.platform_os_stop_in_flight = false;
        Ok(true)
    }

    pub fn begin_platform_vpn_start_for_intent(&self, intent_epoch: u64) -> CoreResult<String> {
        let issuer = crate::platform_owner::current_process_identity()?;
        let mut inner = self.lock()?;
        if intent_epoch == 0 || inner.platform_vpn_intent_epoch != intent_epoch {
            return Err(CoreError::msg(format!(
                "platform VPN start intent {intent_epoch} was superseded"
            )));
        }
        if inner.platform_os_stop_epoch != 0 {
            return Err(CoreError::msg(format!(
                "platform VPN OS stop for attempt {} is still pending confirmation",
                inner.platform_os_stop_attempt_id
            )));
        }
        self.sync_platform_locked(&mut inner);
        if inner.platform_start_outcome == PlatformStartOutcome::Pending {
            return Err(CoreError::msg("platform VPN start is already pending"));
        }
        if !inner.platform_start_attempt_id.is_empty() && !inner.platform_vpn_cleanup_complete {
            return Err(CoreError::msg(
                "previous platform VPN connection cleanup is still pending",
            ));
        }
        if inner.platform_vpn_running {
            return Err(CoreError::msg("platform VPN is already connected"));
        }

        let next_sequence = inner.platform_start_sequence.saturating_add(1);
        let attempt_id = format!("{}-{}", PlatformVpnState::now_nanos(), next_sequence);
        let issuer_lease = {
            let journal_path = platform_owner_journal_path(&inner);
            match crate::platform_owner::read(&journal_path)? {
                JournalRead::Missing => {}
                JournalRead::Present(owner) => {
                    return Err(CoreError::msg(format!(
                    "previous platform VPN owner journal for attempt {} is still pending cleanup",
                    owner.attempt_id
                )))
                }
            }
            let lease = crate::platform_owner::acquire_owner_lease_exact(
                &platform_owner_lease_path(&inner, PlatformVpnOwnerLeaseRole::Issuer),
                platform_owner_lease_record(
                    &attempt_id,
                    issuer.clone(),
                    PlatformVpnOwnerLeaseRole::Issuer,
                ),
            )?;
            crate::platform_owner::create_pending_exact(
                &journal_path,
                PlatformVpnOwnerJournal {
                    attempt_id: attempt_id.clone(),
                    issuer,
                    extension: None,
                    phase: PlatformVpnOwnerPhase::Pending,
                },
            )?;
            lease
        };
        inner.platform_vpn_issuer_lease = Some(issuer_lease);
        inner.platform_vpn_extension_lease = None;
        inner.platform_start_sequence = next_sequence;
        inner.platform_start_attempt_id = attempt_id.clone();
        inner.platform_start_outcome = PlatformStartOutcome::Pending;
        inner.platform_start_delivery_observed = false;
        inner.platform_extension_attached = false;
        inner.platform_stop_requested = false;
        inner.platform_extension_owner_pid = 0;
        inner.platform_extension_owner_start_time = 0;
        inner.platform_vpn_cleanup_complete = false;
        if let Some(handoff) = inner.platform_session_handoff.as_mut() {
            handoff.attempt_id = attempt_id.clone();
            handoff.updated_at = PlatformVpnState::now_nanos();
        }
        inner.platform_vpn_starting = true;
        inner.platform_vpn_running = false;
        inner.platform_remote_state_updated_at = 0;
        inner.platform_remote_state_seen_at = None;
        inner.platform_remote_stale_since = None;
        inner.platform_watchdog_cleanup_recoverable = false;
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
    pub fn bind_platform_vpn_start(&self, attempt_id: &str) -> CoreResult<String> {
        if attempt_id.is_empty() {
            return Err(CoreError::msg("platform VPN start attempt id is empty"));
        }
        let extension_owner = crate::platform_owner::current_process_identity()?;
        let mut inner = self.lock()?;
        self.sync_platform_for_binding_locked(&mut inner, attempt_id)?;
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
        #[cfg(feature = "native-anyconnect")]
        let native_vpn_running = inner.running_native.is_some();
        #[cfg(not(feature = "native-anyconnect"))]
        let native_vpn_running = false;
        if inner.platform_start_outcome == PlatformStartOutcome::Connected && !native_vpn_running {
            // HarmonyOS may recreate the Extension process after a crash and
            // redeliver the same still-connected Want. The fresh native core
            // has no worker, so reopen only this non-terminal attempt's start
            // phase. Failed/Cancelled attempts remain terminal above.
            inner.platform_start_outcome = PlatformStartOutcome::Pending;
            inner.platform_vpn_starting = true;
            inner.platform_vpn_running = false;
            inner.platform_vpn_cleanup_complete = false;
            inner.platform_watchdog_cleanup_recoverable = false;
            self.push_diag_locked(
                &mut inner,
                "info",
                format!("platform VPN extension recovering connected attempt {attempt_id}"),
            );
        }

        {
            let journal_path = platform_owner_journal_path(&inner);
            let journal = match crate::platform_owner::read(&journal_path)? {
                JournalRead::Missing => {
                    return Err(CoreError::msg(format!(
                        "platform VPN owner journal is missing for attempt {attempt_id}"
                    )))
                }
                JournalRead::Present(journal) if journal.attempt_id == attempt_id => journal,
                JournalRead::Present(journal) => {
                    return Err(CoreError::msg(format!(
                    "stale platform VPN start attempt {attempt_id}; owner journal belongs to {}",
                    journal.attempt_id
                )))
                }
            };
            let issuer_lease_path =
                platform_owner_lease_path(&inner, PlatformVpnOwnerLeaseRole::Issuer);
            let extension_lease_path =
                platform_owner_lease_path(&inner, PlatformVpnOwnerLeaseRole::Extension);
            let extension_lease_record = platform_owner_lease_record(
                attempt_id,
                extension_owner.clone(),
                PlatformVpnOwnerLeaseRole::Extension,
            );
            let acquired_extension_lease = match (journal.phase, journal.extension.as_ref()) {
                (PlatformVpnOwnerPhase::Pending, None) => {
                    let issuer_lease_record = platform_owner_lease_record(
                        attempt_id,
                        journal.issuer.clone(),
                        PlatformVpnOwnerLeaseRole::Issuer,
                    );
                    match crate::platform_owner::observe_owner_lease_exact(
                        &issuer_lease_path,
                        &issuer_lease_record,
                    )? {
                        PlatformVpnOwnerLeaseObservation::HeldExact => {}
                        PlatformVpnOwnerLeaseObservation::Released => {
                            return Err(CoreError::msg(format!(
                                "platform VPN start issuer lease for attempt {attempt_id} was released"
                            )))
                        }
                        PlatformVpnOwnerLeaseObservation::HeldOther => {
                            return Err(CoreError::msg(format!(
                                "cannot verify the exact platform VPN start issuer lease for attempt {attempt_id}"
                            )))
                        }
                    }
                    let lease = crate::platform_owner::acquire_owner_lease_exact(
                        &extension_lease_path,
                        extension_lease_record.clone(),
                    )?;
                    crate::platform_owner::upgrade_attached_exact(
                        &journal_path,
                        attempt_id,
                        journal.issuer.clone(),
                        extension_owner.clone(),
                    )?;
                    Some(lease)
                }
                (PlatformVpnOwnerPhase::Attached, Some(current_owner))
                    if current_owner == &extension_owner =>
                {
                    if inner.platform_vpn_extension_lease.is_none() {
                        Some(crate::platform_owner::acquire_owner_lease_exact(
                            &extension_lease_path,
                            extension_lease_record,
                        )?)
                    } else {
                        None
                    }
                }
                (PlatformVpnOwnerPhase::Attached, Some(current_owner)) => {
                    let previous_lease_record = platform_owner_lease_record(
                        attempt_id,
                        current_owner.clone(),
                        PlatformVpnOwnerLeaseRole::Extension,
                    );
                    match crate::platform_owner::observe_owner_lease_exact(
                        &extension_lease_path,
                        &previous_lease_record,
                    )? {
                        PlatformVpnOwnerLeaseObservation::Released => {}
                        PlatformVpnOwnerLeaseObservation::HeldExact => {
                            return Err(CoreError::msg(format!(
                                "platform VPN attempt {attempt_id} is still owned by another Extension process"
                            )))
                        }
                        PlatformVpnOwnerLeaseObservation::HeldOther => {
                            return Err(CoreError::msg(format!(
                                "cannot verify the exact previous Extension lease for attempt {attempt_id}"
                            )))
                        }
                    }
                    let lease = crate::platform_owner::acquire_owner_lease_exact(
                        &extension_lease_path,
                        extension_lease_record,
                    )?;
                    crate::platform_owner::rebind_attached_exact(
                        &journal_path,
                        attempt_id,
                        journal.issuer.clone(),
                        current_owner.clone(),
                        extension_owner.clone(),
                    )?;
                    Some(lease)
                }
                _ => {
                    return Err(CoreError::msg(format!(
                        "platform VPN owner journal has an invalid phase for attempt {attempt_id}"
                    )))
                }
            };
            if let Some(lease) = acquired_extension_lease {
                inner.platform_vpn_extension_lease = Some(lease);
            }
            // Stop publishes before racing Pending→Stopping. Re-read after
            // the journal CAS so a stale pre-CAS frame cannot revive it.
            self.sync_platform_locked(&mut inner);
            if inner.platform_start_attempt_id != attempt_id {
                return Err(CoreError::msg(format!(
                    "platform VPN start attempt {attempt_id} was superseded after owner attachment"
                )));
            }
        }

        let owner_identity_changed = inner.platform_extension_owner_pid != extension_owner.pid
            || inner.platform_extension_owner_start_time != extension_owner.start_time;
        inner.platform_extension_owner_pid = extension_owner.pid;
        inner.platform_extension_owner_start_time = extension_owner.start_time;
        if !inner.platform_extension_attached {
            inner.platform_start_delivery_observed = true;
            inner.platform_extension_attached = true;
            inner.platform_vpn_cleanup_complete = false;
            inner.platform_watchdog_cleanup_recoverable = false;
            self.push_diag_locked(
                &mut inner,
                "info",
                format!("platform VPN extension attached to {attempt_id}"),
            );
            self.persist_platform_locked(&mut inner)?;
        } else if inner.platform_start_outcome == PlatformStartOutcome::Pending
            && inner.platform_vpn_starting
            && !inner.platform_vpn_running
        {
            inner.platform_start_delivery_observed = true;
            self.persist_platform_locked(&mut inner)?;
        } else if owner_identity_changed {
            self.persist_platform_locked(&mut inner)?;
        }
        Ok(if extension_owner.start_time > 0 {
            format!("{}:{}", extension_owner.pid, extension_owner.start_time)
        } else {
            format!("{}:unavailable", extension_owner.pid)
        })
    }

    /// Acknowledge that the exact HarmonyOS VpnConnection owner has finished
    /// every native/platform operation and its destroy Promise has resolved.
    pub fn complete_platform_vpn_cleanup(&self, attempt_id: &str) -> CoreResult<bool> {
        let acknowledging_owner = crate::platform_owner::current_process_identity()?;
        let mut inner = self.lock()?;
        self.sync_platform_locked(&mut inner);
        if attempt_id.is_empty() || inner.platform_start_attempt_id != attempt_id {
            return Ok(false);
        }
        if inner.platform_vpn_running
            || inner.platform_vpn_starting
            || !matches!(
                inner.platform_start_outcome,
                PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
            )
        {
            return Ok(false);
        }
        if inner.platform_vpn_cleanup_complete {
            return Ok(true);
        }

        {
            if inner.platform_vpn_extension_lease.is_none() {
                return Err(CoreError::msg(format!(
                    "platform VPN Extension lease is missing before cleanup acknowledgement for {attempt_id}"
                )));
            }
            let journal_path = platform_owner_journal_path(&inner);
            let journal = match crate::platform_owner::read(&journal_path)? {
                JournalRead::Missing => {
                    return Err(CoreError::msg(format!(
                        "platform VPN owner journal is missing before cleanup acknowledgement for {attempt_id}"
                    )))
                }
                JournalRead::Present(journal) if journal.attempt_id == attempt_id => journal,
                JournalRead::Present(_) => return Ok(false),
            };
            let expected_extension = match (journal.phase, journal.extension) {
                (PlatformVpnOwnerPhase::Attached, Some(extension))
                    if extension == acknowledging_owner
                        && extension.pid == inner.platform_extension_owner_pid
                        && extension.start_time == inner.platform_extension_owner_start_time =>
                {
                    extension
                }
                _ => {
                    return Err(CoreError::msg(format!(
                        "platform VPN owner journal does not match the Extension acknowledging cleanup for {attempt_id}"
                    )))
                }
            };
            if !crate::platform_owner::delete_exact(
                &journal_path,
                attempt_id,
                Some(expected_extension),
            )? {
                return Err(CoreError::msg(format!(
                    "platform VPN owner journal changed before cleanup acknowledgement for {attempt_id}"
                )));
            }

            inner.platform_vpn_cleanup_complete = true;
            inner.platform_extension_attached = false;
            inner.platform_stop_requested = true;
            inner.platform_extension_owner_pid = 0;
            inner.platform_extension_owner_start_time = 0;
            inner.platform_watchdog_cleanup_recoverable = false;
            self.push_diag_locked(
                &mut inner,
                "info",
                format!("platform VPN connection cleanup completed for {attempt_id}"),
            );
            // Exact journal deletion is the cleanup linearization point. Once
            // it succeeds, retaining either lease can self-deadlock the next
            // same-process start.
            inner.platform_vpn_issuer_lease = None;
            inner.platform_vpn_extension_lease = None;
            self.persist_platform_locked(&mut inner)?;
            Ok(true)
        }
    }

    /// Release a cleanup barrier only after HarmonyOS confirms that the
    /// Extension Ability stopped and the exact owner lease was released.
    pub async fn recover_platform_vpn_cleanup_after_confirmed_stop(
        &self,
        attempt_id: &str,
    ) -> CoreResult<bool> {
        if attempt_id.is_empty() {
            return Ok(false);
        }
        let deadline = tokio::time::Instant::now() + PLATFORM_OS_STOP_RECOVERY_DEADLINE;
        loop {
            let (journal_path, issuer_lease_path, extension_lease_path, local_unattached_fence) = {
                let mut inner = self.lock()?;
                self.sync_platform_locked(&mut inner);
                if inner.platform_start_attempt_id == attempt_id
                    && inner.platform_vpn_cleanup_complete
                {
                    return Ok(true);
                }
                let local_unattached_fence = inner.platform_start_attempt_id == attempt_id
                    && !inner.platform_extension_attached
                    && inner.platform_stop_requested
                    && matches!(
                        inner.platform_start_outcome,
                        PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
                    );
                (
                    platform_owner_journal_path(&inner),
                    platform_owner_lease_path(&inner, PlatformVpnOwnerLeaseRole::Issuer),
                    platform_owner_lease_path(&inner, PlatformVpnOwnerLeaseRole::Extension),
                    local_unattached_fence,
                )
            };
            let journal = match crate::platform_owner::read(&journal_path)? {
                JournalRead::Missing => {
                    let mut inner = self.lock()?;
                    self.sync_platform_locked(&mut inner);
                    if inner.platform_start_attempt_id == attempt_id
                        && inner.platform_vpn_cleanup_complete
                    {
                        return Ok(true);
                    }
                    return Err(CoreError::msg(format!(
                        "platform VPN owner journal disappeared before cleanup was confirmed for {attempt_id}"
                    )));
                }
                JournalRead::Present(journal) if journal.attempt_id == attempt_id => journal,
                JournalRead::Present(_) => return Ok(false),
            };
            let (expected_extension, proof, recovery_reason) =
                match (journal.phase, journal.extension.as_ref()) {
                    (PlatformVpnOwnerPhase::Stopping, None) => (
                        None,
                        CleanupRecoveryProof::Proven,
                        "the exact pending owner was durably fenced before confirmed OS stop"
                            .to_owned(),
                    ),
                    (PlatformVpnOwnerPhase::Pending, None) if local_unattached_fence => (
                        None,
                        CleanupRecoveryProof::Proven,
                        "the exact local attempt was terminal before an Extension adopted it"
                            .to_owned(),
                    ),
                    (PlatformVpnOwnerPhase::Pending, None) => {
                        let expected = platform_owner_lease_record(
                            attempt_id,
                            journal.issuer.clone(),
                            PlatformVpnOwnerLeaseRole::Issuer,
                        );
                        let proof = match crate::platform_owner::observe_owner_lease_exact(
                            &issuer_lease_path,
                            &expected,
                        )? {
                            PlatformVpnOwnerLeaseObservation::Released => {
                                CleanupRecoveryProof::Proven
                            }
                            PlatformVpnOwnerLeaseObservation::HeldExact => {
                                CleanupRecoveryProof::OwnerAlive
                            }
                            PlatformVpnOwnerLeaseObservation::HeldOther => {
                                CleanupRecoveryProof::OwnerLivenessUnknown
                            }
                        };
                        (
                            None,
                            proof,
                            format!(
                                "exact pending issuer lease {}:{} was released",
                                journal.issuer.pid, journal.issuer.start_time
                            ),
                        )
                    }
                    (PlatformVpnOwnerPhase::Attached, Some(extension)) => {
                        let expected = platform_owner_lease_record(
                            attempt_id,
                            extension.clone(),
                            PlatformVpnOwnerLeaseRole::Extension,
                        );
                        let proof = match crate::platform_owner::observe_owner_lease_exact(
                            &extension_lease_path,
                            &expected,
                        )? {
                            PlatformVpnOwnerLeaseObservation::Released => {
                                CleanupRecoveryProof::Proven
                            }
                            PlatformVpnOwnerLeaseObservation::HeldExact => {
                                CleanupRecoveryProof::OwnerAlive
                            }
                            PlatformVpnOwnerLeaseObservation::HeldOther => {
                                CleanupRecoveryProof::OwnerLivenessUnknown
                            }
                        };
                        (
                            Some(extension.clone()),
                            proof,
                            format!(
                                "exact Extension owner lease {}:{} was released",
                                extension.pid, extension.start_time
                            ),
                        )
                    }
                    _ => {
                        return Err(CoreError::msg(format!(
                            "platform VPN owner journal has an invalid phase for {attempt_id}"
                        )))
                    }
                };
            match proof {
                CleanupRecoveryProof::Proven => {
                    if !crate::platform_owner::delete_exact(
                        &journal_path,
                        attempt_id,
                        expected_extension,
                    )? {
                        // Ownership changed between proof and delete. Re-read
                        // the exact journal instead of clearing its replacement.
                        continue;
                    }
                    let mut inner = self.lock()?;
                    self.sync_platform_locked(&mut inner);
                    if inner.platform_start_attempt_id == attempt_id {
                        if inner.platform_vpn_cleanup_complete {
                            return Ok(true);
                        }
                        inner.platform_vpn_starting = false;
                        inner.platform_vpn_running = false;
                        if inner.platform_start_outcome != PlatformStartOutcome::Failed {
                            inner.platform_start_outcome = PlatformStartOutcome::Cancelled;
                            self.set_lifecycle_locked(
                                &mut inner,
                                ConnectionLifecycle::Disconnected,
                                None,
                            );
                        }
                        inner.platform_vpn_cleanup_complete = true;
                        inner.platform_extension_attached = false;
                        inner.platform_stop_requested = true;
                        inner.platform_extension_owner_pid = 0;
                        inner.platform_extension_owner_start_time = 0;
                        inner.platform_watchdog_cleanup_recoverable = false;
                        self.push_diag_locked(
                            &mut inner,
                            "warn",
                            format!(
                                "released orphaned platform VPN cleanup barrier for {attempt_id} after confirmed OS stop: {recovery_reason}"
                            ),
                        );
                        inner.platform_vpn_issuer_lease = None;
                        inner.platform_vpn_extension_lease = None;
                        self.persist_platform_locked(&mut inner)?;
                    }
                    return Ok(true);
                }
                CleanupRecoveryProof::OwnerAlive => {
                    if tokio::time::Instant::now() >= deadline {
                        return Ok(false);
                    }
                    tokio::time::sleep(PLATFORM_OS_STOP_RECOVERY_POLL_INTERVAL).await;
                }
                CleanupRecoveryProof::OwnerLivenessUnknown => {
                    let identity = journal.extension.as_ref().unwrap_or(&journal.issuer);
                    return Err(CoreError::msg(format!(
                        "cannot verify the exact platform VPN ownership lease for {}:{}",
                        identity.pid, identity.start_time,
                    )));
                }
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

    pub async fn await_platform_vpn_stop(&self, attempt_id: &str) -> CoreResult<bool> {
        if attempt_id.is_empty() {
            return Ok(true);
        }
        let mut receiver = self.platform_start_tx.subscribe();
        let deadline = tokio::time::Instant::now() + PLATFORM_VPN_START_DEADLINE;
        loop {
            let stopped = {
                let mut inner = self.lock()?;
                self.sync_platform_locked(&mut inner);
                if inner.platform_start_attempt_id != attempt_id {
                    return Ok(false);
                }
                !inner.platform_vpn_running
                    && !inner.platform_vpn_starting
                    && inner.platform_vpn_cleanup_complete
                    && matches!(
                        inner.platform_start_outcome,
                        PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
                    )
            };
            if stopped {
                return Ok(true);
            }
            tokio::select! {
                changed = receiver.changed() => {
                    changed.map_err(|_| CoreError::msg(
                        "platform VPN stop coordinator closed"
                    ))?;
                }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                _ = tokio::time::sleep_until(deadline) => {
                    return Err(CoreError::msg(format!(
                        "platform VPN attempt {attempt_id} did not stop before the cleanup deadline"
                    )));
                }
            }
        }
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
        let mut inner = self.lock()?;
        self.sync_platform_locked(&mut inner);
        if !self.platform_start_is_pending_locked(&inner, attempt_id)
            || (require_unattached && inner.platform_extension_attached)
        {
            return Ok(false);
        }
        if require_unattached {
            let journal_path = platform_owner_journal_path(&inner);
            if !crate::platform_owner::delete_pending_exact(&journal_path, attempt_id)? {
                // Attachment or a durable stop fence may have won before its
                // shared-memory frame arrived. Do not waive their cleanup.
                return Ok(false);
            }
        }
        #[cfg(feature = "native-anyconnect")]
        let stop_native = self.apply_platform_vpn_failed_locked(&mut inner, error)?;
        #[cfg(not(feature = "native-anyconnect"))]
        self.apply_platform_vpn_failed_locked(&mut inner, error)?;
        inner.platform_start_outcome = PlatformStartOutcome::Failed;
        if require_unattached {
            inner.platform_vpn_cleanup_complete = true;
        }
        if inner.platform_vpn_cleanup_complete {
            inner.platform_vpn_issuer_lease = None;
            inner.platform_vpn_extension_lease = None;
        }
        self.persist_platform_locked(&mut inner)?;
        drop(inner);
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

    /// Ask the exact attached Extension owner to tear down its native worker
    /// and VpnConnection while its process is still alive.
    pub fn request_platform_vpn_stop(&self, attempt_id: &str) -> CoreResult<bool> {
        if attempt_id.is_empty() {
            return Ok(false);
        }
        let mut inner = self.lock()?;
        self.sync_platform_locked(&mut inner);
        if inner.platform_start_attempt_id != attempt_id {
            return Ok(false);
        }
        if inner.platform_vpn_cleanup_complete {
            return Ok(true);
        }
        if !inner.platform_extension_attached {
            return Ok(false);
        }
        if !inner.platform_stop_requested {
            inner.platform_stop_requested = true;
            self.push_diag_locked(
                &mut inner,
                "info",
                format!("platform VPN cooperative stop requested for {attempt_id}"),
            );
            self.persist_platform_locked(&mut inner)?;
        }
        Ok(true)
    }

    /// Atomically fence and return the exact platform owner that still owes
    /// cleanup. A Pending attempt becomes terminal before a late Want can
    /// attach; an attached owner remains authoritative until real teardown.
    pub fn claim_current_platform_vpn_stop(&self, intent_epoch: u64) -> CoreResult<String> {
        let mut inner = self.lock()?;
        if intent_epoch == 0 || inner.platform_vpn_intent_epoch != intent_epoch {
            return Err(CoreError::msg(format!(
                "platform VPN stop intent {intent_epoch} was superseded"
            )));
        }
        if inner.platform_os_stop_in_flight {
            return Err(CoreError::msg(format!(
                "platform VPN OS stop for attempt {} is already in flight",
                inner.platform_os_stop_attempt_id
            )));
        }
        let retained_stop_attempt =
            (inner.platform_os_stop_epoch != 0).then(|| inner.platform_os_stop_attempt_id.clone());
        self.sync_platform_locked(&mut inner);
        let journal_path = platform_owner_journal_path(&inner);
        let journal = match crate::platform_owner::read(&journal_path)? {
            JournalRead::Present(journal) => {
                if retained_stop_attempt
                    .as_ref()
                    .is_some_and(|attempt| attempt != &journal.attempt_id)
                {
                    return Err(CoreError::msg(format!(
                        "platform VPN OS stop fence owns {} while the owner journal owns {}",
                        retained_stop_attempt.as_deref().unwrap_or_default(),
                        journal.attempt_id
                    )));
                }
                if !inner.platform_start_attempt_id.is_empty()
                    && !inner.platform_vpn_cleanup_complete
                    && inner.platform_start_attempt_id != journal.attempt_id
                {
                    return Err(CoreError::msg(format!(
                        "platform VPN state owns {} while the owner journal owns {}",
                        inner.platform_start_attempt_id, journal.attempt_id
                    )));
                }
                journal
            }
            JournalRead::Missing => {
                if !inner.platform_start_attempt_id.is_empty()
                    && !inner.platform_vpn_cleanup_complete
                {
                    return Err(CoreError::msg(format!(
                        "platform VPN owner journal is missing for active attempt {}",
                        inner.platform_start_attempt_id
                    )));
                }
                if let Some(attempt_id) = retained_stop_attempt {
                    inner.platform_os_stop_epoch = intent_epoch;
                    inner.platform_os_stop_attempt_id = attempt_id.clone();
                    inner.platform_os_stop_in_flight = false;
                    return Ok(attempt_id);
                }
                return Ok(String::new());
            }
        };
        let attempt_id = journal.attempt_id.clone();
        if inner.platform_start_attempt_id.is_empty() || inner.platform_vpn_cleanup_complete {
            self.push_diag_locked(
                &mut inner,
                "info",
                format!("platform VPN stop claimed cold owner journal {attempt_id}"),
            );
        } else {
            if inner.platform_start_outcome == PlatformStartOutcome::Pending {
                inner.platform_start_outcome = PlatformStartOutcome::Cancelled;
                inner.platform_vpn_starting = false;
                inner.platform_vpn_running = false;
            }
            inner.platform_stop_requested = true;
            self.push_diag_locked(
                &mut inner,
                "info",
                format!("platform VPN stop claimed exact owner {attempt_id}"),
            );
            // Make the stop state visible before Pending races Attached.
            self.persist_platform_locked(&mut inner)?;
        }
        match (journal.phase, journal.extension.as_ref()) {
            (PlatformVpnOwnerPhase::Stopping, None)
            | (PlatformVpnOwnerPhase::Attached, Some(_)) => {}
            (PlatformVpnOwnerPhase::Pending, None) => {
                if crate::platform_owner::fence_pending_stop_exact(
                    &journal_path,
                    &attempt_id,
                    journal.issuer.clone(),
                )? {
                    self.push_diag_locked(
                        &mut inner,
                        "info",
                        format!("platform VPN pending owner durably fenced for stop {attempt_id}"),
                    );
                } else {
                    match crate::platform_owner::read(&journal_path)? {
                        JournalRead::Present(current)
                            if current.attempt_id == attempt_id
                                && matches!(
                                    (current.phase, current.extension.as_ref()),
                                    (PlatformVpnOwnerPhase::Stopping, None)
                                        | (PlatformVpnOwnerPhase::Attached, Some(_))
                                ) => {}
                        JournalRead::Present(current) => {
                            return Err(CoreError::msg(format!(
                                "platform VPN owner changed from {attempt_id} to {} while claiming stop",
                                current.attempt_id
                            )))
                        }
                        JournalRead::Missing => {
                            return Err(CoreError::msg(format!(
                                "platform VPN owner journal disappeared while claiming stop for {attempt_id}"
                            )))
                        }
                    }
                }
            }
            _ => {
                return Err(CoreError::msg(format!(
                    "platform VPN owner journal has an invalid phase while claiming stop for {attempt_id}"
                )))
            }
        }
        inner.platform_os_stop_epoch = intent_epoch;
        inner.platform_os_stop_attempt_id = attempt_id.clone();
        inner.platform_os_stop_in_flight = false;
        Ok(attempt_id)
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

    pub fn set_platform_vpn_starting_for_attempt(
        &self,
        attempt_id: &str,
        starting: bool,
    ) -> CoreResult<bool> {
        let mut inner = self.lock()?;
        self.sync_platform_locked(&mut inner);
        if !platform_attempt_accepts_updates(&inner, attempt_id) {
            return Ok(false);
        }
        if starting && inner.platform_start_outcome != PlatformStartOutcome::Pending {
            return Ok(false);
        }
        inner.platform_vpn_starting = starting;
        if starting {
            inner.platform_vpn_running = false;
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Establishing, None);
        }
        self.persist_platform_locked(&mut inner)?;
        Ok(true)
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

    pub fn set_platform_vpn_failed_for_attempt(
        &self,
        attempt_id: &str,
        error: String,
    ) -> CoreResult<bool> {
        #[cfg(feature = "native-anyconnect")]
        let stop_native = {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            if !platform_attempt_accepts_updates(&inner, attempt_id) {
                return Ok(false);
            }
            let stop = self.apply_platform_vpn_failed_locked(&mut inner, error)?;
            self.persist_platform_locked(&mut inner)?;
            stop
        };
        #[cfg(not(feature = "native-anyconnect"))]
        {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            if !platform_attempt_accepts_updates(&inner, attempt_id) {
                return Ok(false);
            }
            self.apply_platform_vpn_failed_locked(&mut inner, error)?;
            self.persist_platform_locked(&mut inner)?;
        }
        #[cfg(feature = "native-anyconnect")]
        if let Some(session) = stop_native {
            session.cancel();
            let _ = session.join(Duration::from_secs(2));
        }
        Ok(true)
    }

    /// Publish an Extension heartbeat from the exact native owner lifecycle.
    pub fn extension_tick(&self, attempt_id: &str) -> CoreResult<String> {
        let snapshot = self.tick()?;
        let inner = self.lock()?;
        if inner.platform_start_attempt_id != attempt_id {
            return Err(CoreError::msg(format!(
                "stale platform VPN heartbeat for attempt {attempt_id}"
            )));
        }
        if inner.platform_stop_requested {
            return Ok("stopping".to_owned());
        }
        if matches!(
            inner.platform_start_outcome,
            PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
        ) {
            return Ok(
                if inner.platform_start_outcome == PlatformStartOutcome::Failed {
                    "failed"
                } else {
                    "disconnected"
                }
                .to_owned(),
            );
        }
        if !inner.platform_extension_attached {
            return Err(CoreError::msg(format!(
                "platform VPN heartbeat has no owner for attempt {attempt_id}"
            )));
        }
        Ok(snapshot.lifecycle.as_str().to_owned())
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

    pub(super) fn refresh_platform_vpn_owner_liveness_locked(
        &self,
        inner: &mut Inner,
    ) -> CoreResult<()> {
        if !inner.platform_extension_attached
            || inner.platform_vpn_cleanup_complete
            || !matches!(
                inner.platform_start_outcome,
                PlatformStartOutcome::Pending | PlatformStartOutcome::Connected
            )
        {
            return Ok(());
        }
        let Some(_released_lease) = crate::platform_owner::lock_released_owner_lease(
            &platform_owner_lease_path(inner, PlatformVpnOwnerLeaseRole::Extension),
        )?
        else {
            return Ok(());
        };
        let JournalRead::Present(journal) =
            crate::platform_owner::read(&platform_owner_journal_path(inner))?
        else {
            // Cooperative cleanup deletes the journal before releasing the
            // lease. Its terminal IPC frame may still be on the way.
            return Ok(());
        };
        if journal.attempt_id != inner.platform_start_attempt_id
            || journal.phase != PlatformVpnOwnerPhase::Attached
            || !journal.extension.as_ref().is_some_and(|owner| {
                owner.pid == inner.platform_extension_owner_pid
                    && owner.start_time == inner.platform_extension_owner_start_time
            })
        {
            return Ok(());
        }

        const MESSAGE: &str = "VPN extension owner lease was released";
        inner.platform_vpn_running = false;
        inner.platform_vpn_starting = false;
        inner.platform_start_outcome = PlatformStartOutcome::Failed;
        inner.platform_remote_stale_since = None;
        inner.connected_at = None;
        self.set_lifecycle_locked(inner, ConnectionLifecycle::Failed, Some(MESSAGE.to_owned()));
        self.push_diag_locked(inner, "error", MESSAGE);
        // Keep the exact released-lease guard until the terminal ashmem frame
        // is committed so a replacement cannot rewrite ownership mid-check.
        self.persist_platform_locked(inner)
    }

    /// Read the newest sibling-process frame from the opposite ashmem lane.
    pub(super) fn sync_platform_locked(&self, inner: &mut Inner) -> bool {
        self.sync_platform_locked_with_adoption(inner, None)
    }

    fn sync_platform_for_binding_locked(
        &self,
        inner: &mut Inner,
        attempt_id: &str,
    ) -> CoreResult<bool> {
        let Some(platform) = self.platform_ipc()? else {
            // Keep the coordinator independently testable; production
            // Extension calls always attach ashmem immediately before bind.
            return Ok(false);
        };
        if platform.is_ui() {
            return Err(CoreError::msg(
                "platform VPN start binding is only valid in the Extension",
            ));
        }
        let envelope = platform
            .read_remote()
            .map_err(platform_ipc_error)?
            .ok_or_else(|| CoreError::msg("platform VPN start has no UI state"))?;
        validate_platform_start_envelope(&envelope, attempt_id)?;
        Ok(self.apply_platform_envelope_locked(inner, false, Some(attempt_id), envelope))
    }

    fn sync_platform_locked_with_adoption(
        &self,
        inner: &mut Inner,
        expected_extension_attempt_id: Option<&str>,
    ) -> bool {
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
        self.apply_platform_envelope_locked(
            inner,
            platform.is_ui(),
            expected_extension_attempt_id,
            envelope,
        )
    }

    pub(super) fn apply_platform_envelope_locked(
        &self,
        inner: &mut Inner,
        is_ui: bool,
        expected_extension_attempt_id: Option<&str>,
        envelope: crate::platform_ipc::PlatformEnvelope,
    ) -> bool {
        let browser_request_acknowledged = !is_ui
            && acknowledge_platform_browser_request_locked(
                inner,
                envelope.browser_request_ack.as_deref(),
            );
        let Some(remote) = envelope.state else {
            return browser_request_acknowledged;
        };
        let mut remote_attempt_matches = !remote.start_attempt_id.is_empty()
            && remote.start_attempt_id == inner.platform_start_attempt_id;

        // The UI owns transaction creation and only accepts replies for its
        // current id. The extension adopts a new id from the UI lane when the
        // corresponding Want is attached. Neither side may regress a terminal
        // outcome back to Pending.
        if is_ui && !remote_attempt_matches {
            return browser_request_acknowledged;
        }

        // The Extension must not acquire a transaction merely because a
        // normal heartbeat/sync observed a newer UI lane. In particular, an
        // old session's asynchronous stop must remain scoped to its old
        // attempt. Ownership changes only while handling the matching Want in
        // bind_platform_vpn_start.
        let adopted_attempt = !is_ui
            && !remote_attempt_matches
            && expected_extension_attempt_id
                .is_some_and(|attempt_id| remote.start_attempt_id == attempt_id)
            && !remote.start_attempt_id.is_empty();
        if !is_ui && !remote_attempt_matches && !adopted_attempt {
            return browser_request_acknowledged;
        }
        if adopted_attempt {
            inner.generation = inner.generation.saturating_add(1);
            inner.platform_start_attempt_id = remote.start_attempt_id.clone();
            inner.platform_start_outcome = remote.start_outcome;
            inner.platform_start_delivery_observed = remote.delivery_observed;
            inner.platform_extension_attached = remote.extension_attached;
            inner.platform_stop_requested = remote.stop_requested;
            inner.platform_extension_owner_pid = remote.extension_owner_pid;
            inner.platform_extension_owner_start_time = remote.extension_owner_start_time;
            inner.platform_vpn_cleanup_complete = remote.cleanup_complete;
            if remote.starting && remote.start_outcome == PlatformStartOutcome::Pending {
                inner.platform_vpn_starting = true;
                inner.platform_vpn_running = false;
                self.set_lifecycle_locked(inner, ConnectionLifecycle::Establishing, None);
            }
            remote_attempt_matches = true;
        }

        let now = Instant::now();
        // Older frames must not refresh liveness or reapply stale state.
        let remote_advanced = remote.updated_at > inner.platform_remote_state_updated_at;
        if remote_advanced {
            inner.platform_remote_state_updated_at = remote.updated_at;
            inner.platform_remote_state_seen_at = Some(now);
            inner.platform_remote_stale_since = None;
        }

        // The UI-side native Client exists only to authenticate and publish a
        // resumable handoff. Once the matching Extension has accepted that
        // handoff (or published a terminal reply), retaining it makes the next
        // automatic reconnect fail with `native session already active`.
        // Never clear the Extension lane's pending Client: it still owns CSTP
        // until attach_tun moves it into running_native.
        #[cfg(feature = "native-anyconnect")]
        if is_ui
            && remote_attempt_matches
            && (remote.extension_attached
                || remote.running
                || matches!(
                    remote.start_outcome,
                    PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
                ))
        {
            inner.pending_native = None;
        }

        // UI watchdog: measure a frozen Extension revision with process-local
        // monotonic time. The second grace interval is intentionally started
        // only after staleness is first observed, so waking from device sleep
        // gives the healthy Extension a full heartbeat period to refresh.
        if heartbeat_watchdog_expired(
            inner,
            now,
            is_ui,
            remote.running,
            remote_attempt_matches,
            remote_advanced,
        ) {
            const MESSAGE: &str = "VPN extension heartbeat stopped";
            inner.platform_vpn_running = false;
            inner.platform_vpn_starting = false;
            inner.platform_start_outcome = PlatformStartOutcome::Failed;
            inner.platform_remote_stale_since = None;
            inner.platform_watchdog_cleanup_recoverable = true;
            self.set_lifecycle_locked(inner, ConnectionLifecycle::Failed, Some(MESSAGE.to_owned()));
            self.push_diag_locked(inner, "error", MESSAGE);
            // Publish the watchdog verdict to the UI lane immediately;
            // the Extension consumes it to destroy an orphaned TUN.
            let _ = self.persist_platform_locked(inner);
            return browser_request_acknowledged;
        }

        if !remote_advanced && !adopted_attempt {
            return browser_request_acknowledged;
        }
        if remote_attempt_matches {
            // Stop intent is UI-owned and sticky for one exact attempt. The
            // Extension owns worker/process proof and publishes it back.
            inner.platform_stop_requested |= remote.stop_requested;
            if is_ui {
                inner.platform_extension_owner_pid = remote.extension_owner_pid;
                inner.platform_extension_owner_start_time = remote.extension_owner_start_time;
            }
        }

        let was_running = inner.platform_vpn_running;
        let was_starting = inner.platform_vpn_starting;
        let local_terminal = matches!(
            inner.platform_start_outcome,
            PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
        );
        let remote_terminal = remote_attempt_matches
            && matches!(
                remote.start_outcome,
                PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
            );
        if remote_terminal {
            inner.platform_start_outcome = remote.start_outcome;
            inner.platform_vpn_starting = false;
            inner.platform_vpn_running = false;
            inner.platform_vpn_cleanup_complete = remote.cleanup_complete;
        } else if !local_terminal {
            inner.platform_vpn_running = remote.running;
            inner.platform_vpn_starting = !remote.running && remote.starting;
            // Cleanup is meaningful only after a terminal owner transition.
            // A malformed non-terminal frame cannot open the next-start
            // barrier while the connection is still live.
            inner.platform_vpn_cleanup_complete = false;
            if remote.running && inner.platform_start_outcome == PlatformStartOutcome::Pending {
                inner.platform_start_outcome = PlatformStartOutcome::Connected;
            }
        }
        if remote_terminal {
            inner.platform_extension_attached = remote.extension_attached;
        } else if remote_attempt_matches && remote.extension_attached {
            inner.platform_extension_attached = true;
        }
        if remote_attempt_matches && remote.delivery_observed {
            inner.platform_start_delivery_observed = true;
        }
        if inner.platform_vpn_cleanup_complete {
            inner.platform_vpn_issuer_lease = None;
            inner.platform_vpn_extension_lease = None;
        }

        #[cfg(feature = "native-anyconnect")]
        let local_mainloop = inner.running_native.as_ref().is_some_and(|session| {
            session.attempt_id() == inner.platform_start_attempt_id
                && session.generation() == inner.generation
        });
        #[cfg(not(feature = "native-anyconnect"))]
        let local_mainloop = false;

        let running = !remote_terminal && (local_mainloop || inner.platform_vpn_running);
        let starting = !running && inner.platform_vpn_starting;
        if !running
            && inner.platform_start_outcome == PlatformStartOutcome::Pending
            && (matches!(remote.lifecycle, ConnectionLifecycle::Failed)
                || remote.last_error.is_some())
        {
            inner.platform_start_outcome = PlatformStartOutcome::Failed;
        }
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
            // The Extension is authoritative even when byte counters are
            // unchanged; connected time and a counter reset must still land.
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
            if matches!(
                inner.snapshot.lifecycle,
                ConnectionLifecycle::Connecting
                    | ConnectionLifecycle::Authenticating
                    | ConnectionLifecycle::Establishing
            ) {
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
        } else if remote_attempt_matches && remote.start_outcome == PlatformStartOutcome::Cancelled
        {
            if !matches!(inner.snapshot.lifecycle, ConnectionLifecycle::Failed) {
                self.set_lifecycle_locked(inner, ConnectionLifecycle::Disconnected, None);
                inner.connected_at = None;
            }
        } else if remote.start_outcome == PlatformStartOutcome::Failed
            || matches!(remote.lifecycle, ConnectionLifecycle::Failed)
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
        } else if remote_attempt_matches
            && remote.start_outcome != PlatformStartOutcome::Pending
            && matches!(remote.lifecycle, ConnectionLifecycle::Disconnected)
            && (was_running || was_starting)
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
        inner.platform_local_state_updated_at = PlatformVpnState::now_nanos()
            .max(inner.platform_local_state_updated_at.saturating_add(1));
        let Some(platform) = self.platform_ipc()? else {
            self.notify_platform_start_locked(inner);
            return Ok(());
        };
        let state = PlatformVpnState {
            start_attempt_id: inner.platform_start_attempt_id.clone(),
            start_outcome: inner.platform_start_outcome,
            delivery_observed: inner.platform_start_delivery_observed,
            extension_attached: inner.platform_extension_attached,
            stop_requested: inner.platform_stop_requested,
            extension_owner_pid: inner.platform_extension_owner_pid,
            extension_owner_start_time: inner.platform_extension_owner_start_time,
            cleanup_complete: inner.platform_vpn_cleanup_complete,
            starting: inner.platform_vpn_starting,
            running: inner.platform_vpn_running || {
                #[cfg(feature = "native-anyconnect")]
                {
                    inner.running_native.as_ref().is_some_and(|session| {
                        session.attempt_id() == inner.platform_start_attempt_id
                            && session.generation() == inner.generation
                    })
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
            updated_at: inner.platform_local_state_updated_at,
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
            delivery_observed: inner.platform_start_delivery_observed,
            extension_attached: inner.platform_extension_attached,
            cleanup_complete: inner.platform_vpn_cleanup_complete,
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

fn platform_owner_journal_path(inner: &Inner) -> PathBuf {
    inner.home.join("runtime/platform-vpn-owner.json")
}

fn platform_owner_lease_path(inner: &Inner, role: PlatformVpnOwnerLeaseRole) -> PathBuf {
    let file_name = match role {
        PlatformVpnOwnerLeaseRole::Issuer => "platform-vpn-owner.issuer.lease",
        PlatformVpnOwnerLeaseRole::Extension => "platform-vpn-owner.extension.lease",
    };
    inner.home.join("runtime").join(file_name)
}

fn platform_owner_lease_record(
    attempt_id: &str,
    identity: ProcessIdentity,
    role: PlatformVpnOwnerLeaseRole,
) -> PlatformVpnOwnerLeaseRecord {
    PlatformVpnOwnerLeaseRecord {
        attempt_id: attempt_id.to_owned(),
        identity,
        role,
    }
}

fn platform_attempt_accepts_updates(inner: &Inner, attempt_id: &str) -> bool {
    !attempt_id.is_empty()
        && inner.platform_start_attempt_id == attempt_id
        && inner.platform_extension_attached
        && matches!(
            inner.platform_start_outcome,
            PlatformStartOutcome::Pending | PlatformStartOutcome::Connected
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CleanupRecoveryProof {
    Proven,
    OwnerAlive,
    OwnerLivenessUnknown,
}

fn acknowledge_terminal_delivery_state(state: &mut PlatformVpnState, attempt_id: &str) -> bool {
    if attempt_id.is_empty()
        || state.start_attempt_id != attempt_id
        || !matches!(
            state.start_outcome,
            PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
        )
    {
        return false;
    }
    state.delivery_observed = true;
    // Delivery is not attachment, connection, or cleanup. Preserve the
    // terminal UI owner and publish only the evidence needed for exact stop.
    state.extension_attached = false;
    state.starting = false;
    state.running = false;
    state.updated_at = PlatformVpnState::now_nanos().max(state.updated_at.saturating_add(1));
    true
}

pub(super) fn validate_platform_start_envelope(
    envelope: &crate::platform_ipc::PlatformEnvelope,
    attempt_id: &str,
) -> CoreResult<()> {
    let remote = envelope
        .state
        .as_ref()
        .ok_or_else(|| CoreError::msg("platform VPN start has no UI state"))?;
    if remote.start_attempt_id != attempt_id {
        return Err(CoreError::msg(format!(
            "stale platform VPN start attempt {attempt_id}; UI owns {}",
            remote.start_attempt_id
        )));
    }
    if matches!(
        remote.start_outcome,
        PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
    ) {
        return Err(CoreError::msg(format!(
            "platform VPN start attempt {attempt_id} is already terminal"
        )));
    }
    Ok(())
}

pub(super) fn heartbeat_watchdog_expired(
    inner: &mut Inner,
    now: Instant,
    is_ui: bool,
    remote_running: bool,
    remote_attempt_matches: bool,
    remote_advanced: bool,
) -> bool {
    let heartbeat_frozen = is_ui
        && inner.platform_vpn_running
        && remote_running
        && remote_attempt_matches
        && !remote_advanced
        && inner
            .platform_remote_state_seen_at
            .is_some_and(|seen| now.duration_since(seen) >= PLATFORM_HEARTBEAT_STALE_AFTER);
    if heartbeat_frozen {
        let stale_since = inner.platform_remote_stale_since.get_or_insert(now);
        return now.duration_since(*stale_since) >= PLATFORM_HEARTBEAT_WAKE_GRACE;
    }
    if !remote_running || remote_advanced {
        inner.platform_remote_stale_since = None;
    }
    false
}
