use super::*;

impl SessionEngine {
    /// Authenticate (anyconnect-rs when enabled) and produce VPN options.
    /// The Harmony shell must then start VpnExtensionAbility and pass the TUN fd
    /// to [`SessionEngine::attach_tun`].
    pub async fn prepare_connect(&self, request: ConnectRequest) -> CoreResult<VpnOptions> {
        request.profile.validate().map_err(CoreError::msg)?;
        let generation = {
            let mut inner = self.lock()?;
            if inner.snapshot.lifecycle.is_busy() {
                return Err(CoreError::msg("connection already in progress"));
            }
            #[cfg(feature = "native-anyconnect")]
            if inner.running_native.is_some() || inner.pending_native.is_some() {
                return Err(CoreError::msg("native session already active"));
            }
            // Fresh interactive-auth session for this connect attempt.
            self.auth.begin_session();
            inner.snapshot.pending_auth = None;
            inner.dry_run = request.dry_run;
            // Reset the UI lane for a fresh ashmem session attempt.
            inner.platform_vpn_running = false;
            inner.platform_vpn_starting = false;
            inner.platform_start_attempt_id.clear();
            inner.platform_start_outcome = PlatformStartOutcome::Idle;
            inner.platform_extension_attached = false;
            inner.platform_session_handoff = None;
            inner.generation += 1;
            let generation = inner.generation;
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Connecting, None);
            inner.snapshot.stats = SessionStats::default();
            self.push_diag_locked(
                &mut inner,
                "info",
                format!(
                    "prepare connect to {} (backend={BACKEND}, dry_run={})",
                    request.profile.server, request.dry_run
                ),
            );
            self.persist_platform_locked(&mut inner)?;
            generation
        };

        let result = if request.dry_run {
            self.prepare_dry_run(&request.profile).await
        } else {
            self.prepare_native(&request.profile).await
        };

        let mut inner = self.lock()?;
        if inner.generation != generation {
            return Err(CoreError::msg("stale connect generation"));
        }
        match result {
            Ok(prepared) => {
                inner.snapshot.pending_auth = None;
                inner.snapshot.network = prepared.network.clone();
                inner.last_vpn_options = prepared.options.clone();
                inner.snapshot.stats.assigned_ip = prepared
                    .network
                    .address
                    .clone()
                    .unwrap_or_else(|| "pending".to_owned());
                inner.snapshot.stats.gateway = prepared
                    .network
                    .gateway
                    .clone()
                    .unwrap_or_else(|| request.profile.server.clone());
                inner.snapshot.stats.mtu = if prepared.network.mtu > 0 {
                    prepared.network.mtu as u32
                } else {
                    prepared.options.mtu
                };
                #[cfg(feature = "native-anyconnect")]
                {
                    inner.pending_native = prepared.pending;
                }
                inner.platform_vpn_starting = true;
                self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Establishing, None);
                self.push_diag_locked(
                    &mut inner,
                    "info",
                    format!(
                        "auth ready; assigned={:?} mtu={}",
                        prepared.network.address, prepared.options.mtu
                    ),
                );
                // Publish the full cookie and credentials only through ashmem.
                // begin_platform_vpn_start binds this payload to its attempt id.
                inner.platform_session_handoff = Some(SessionHandoff {
                    attempt_id: String::new(),
                    options: prepared.options.clone(),
                    network: prepared.network.clone(),
                    updated_at: PlatformVpnState::now_nanos(),
                });
                self.persist_platform_locked(&mut inner)?;
                // The Want is only a platform configuration fallback. Never
                // duplicate the authenticated cookie or credentials into it.
                Ok(sanitized_want_options(&prepared.options))
            }
            Err(err) => {
                let message = err.to_string();
                self.auth.abort();
                inner.snapshot.pending_auth = None;
                inner.platform_vpn_starting = false;
                inner.platform_vpn_running = false;
                #[cfg(feature = "native-anyconnect")]
                {
                    inner.pending_native = None;
                }
                self.set_lifecycle_locked(
                    &mut inner,
                    ConnectionLifecycle::Failed,
                    Some(message.clone()),
                );
                self.push_diag_locked(&mut inner, "error", message.clone());
                // Publish the terminal state so the UI does not remain in
                // "connecting" when authentication or negotiation fails.
                let _ = self.persist_platform_locked(&mut inner);
                let _ = write_last_error(&inner.home, &message);
                Err(err)
            }
        }
    }

    /// VPN-extension process: resume the UI-authenticated cookie, establish
    /// CSTP, keep the Client as `pending_native`, and return the live network
    /// configuration used to create the system TUN.
    fn session_handoff_from_ashmem(&self) -> CoreResult<SessionHandoff> {
        let attempt_id = self.lock()?.platform_start_attempt_id.clone();
        let platform = self
            .platform_ipc()?
            .ok_or_else(|| CoreError::msg("authenticated session ashmem is not attached"))?;
        if platform.is_ui() {
            return Err(CoreError::msg(
                "authenticated session handoff is only readable by the VPN Extension",
            ));
        }
        let envelope = platform
            .read_remote()
            .map_err(platform_ipc_error)?
            .ok_or_else(|| CoreError::msg("authenticated session ashmem has no UI frame"))?;
        let handoff = envelope
            .session_handoff
            .filter(|handoff| handoff.is_valid_for(&attempt_id))
            .ok_or_else(|| {
                CoreError::msg(format!(
                    "authenticated session ashmem handoff is missing or stale for attempt {attempt_id}"
                ))
            })?;
        Ok(handoff)
    }

    pub async fn prepare_in_extension(&self, _options_json: &str) -> CoreResult<String> {
        #[cfg(feature = "native-anyconnect")]
        {
            let handoff = self.session_handoff_from_ashmem()?;
            let attempt_id = handoff.attempt_id;
            let options = handoff.options;
            let generation = {
                let mut inner = self.lock()?;
                inner.generation = inner.generation.saturating_add(1);
                inner.generation
            };
            let pending = tokio::task::spawn_blocking(move || resume_from_options(&options))
                .await
                .map_err(|err| CoreError::msg(format!("extension prepare join: {err}")))??;

            let mut inner = self.lock()?;
            if inner.generation != generation
                || inner.platform_start_attempt_id != attempt_id
                || matches!(
                    inner.platform_start_outcome,
                    PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
                )
            {
                return Err(CoreError::msg("stale extension prepare generation"));
            }
            inner.snapshot.network = pending.network.clone();
            inner.last_vpn_options = pending.options.clone();
            inner.snapshot.stats.assigned_ip = pending
                .network
                .address
                .clone()
                .unwrap_or_else(|| "pending".to_owned());
            inner.snapshot.stats.gateway = pending.network.gateway.clone().unwrap_or_default();
            inner.snapshot.stats.mtu = if pending.network.mtu > 0 {
                pending.network.mtu as u32
            } else {
                pending.options.mtu
            };
            let json = serde_json::to_string(&pending.options)?;
            // Keep Client alive until attach_tun(fd) in this same process.
            inner.pending_native = Some(pending);
            inner.platform_vpn_starting = true;
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Establishing, None);
            self.persist_platform_locked(&mut inner)?;
            Ok(json)
        }
        #[cfg(not(feature = "native-anyconnect"))]
        {
            Err(CoreError::msg(
                "prepare_in_extension requires native-anyconnect",
            ))
        }
    }

    /// Hand the platform TUN fd to OpenConnect and start the packet mainloop.
    ///
    /// On HarmonyOS the VPN extension runs in a **separate process**.
    /// [`SessionEngine::prepare_in_extension`] resumes the authenticated cookie
    /// in that process before this method attaches the platform TUN.
    pub async fn attach_tun(&self, fd: i32, _options_json: &str) -> CoreResult<()> {
        #[cfg(feature = "native-anyconnect")]
        {
            let (pending, generation, attempt_id) = {
                let mut inner = self.lock()?;
                inner.generation = inner.generation.saturating_add(1);
                (
                    inner.pending_native.take(),
                    inner.generation,
                    inner.platform_start_attempt_id.clone(),
                )
            };

            let pending = if let Some(pending) = pending {
                pending
            } else {
                let options = self.session_handoff_from_ashmem()?.options;
                tokio::task::spawn_blocking(move || resume_from_options(&options))
                    .await
                    .map_err(|err| CoreError::msg(format!("resume worker join failed: {err}")))??
            };

            {
                let inner = self.lock()?;
                if inner.generation != generation
                    || inner.platform_start_attempt_id != attempt_id
                    || matches!(
                        inner.platform_start_outcome,
                        PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
                    )
                {
                    return Err(CoreError::msg("stale TUN attach generation"));
                }
            }

            let network = pending.network.clone();
            let options = pending.options.clone();
            let spawn_attempt_id = attempt_id.clone();
            let running = tokio::task::spawn_blocking(move || {
                spawn_mainloop(pending, fd, generation, spawn_attempt_id)
            })
            .await
            .map_err(|err| CoreError::msg(format!("attach worker join failed: {err}")))??;

            let mut inner = self.lock()?;
            if inner.generation != generation
                || inner.platform_start_attempt_id != attempt_id
                || !matches!(
                    inner.snapshot.lifecycle,
                    ConnectionLifecycle::Connecting
                        | ConnectionLifecycle::Authenticating
                        | ConnectionLifecycle::Establishing
                )
                || matches!(
                    inner.platform_start_outcome,
                    PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
                )
            {
                drop(inner);
                running.cancel();
                let _ = running.join(Duration::from_secs(2));
                return Err(CoreError::msg(
                    "OpenConnect mainloop exited during TUN attach",
                ));
            }
            inner.snapshot.network = network.clone();
            inner.last_vpn_options = options;
            inner.snapshot.stats.assigned_ip = network
                .address
                .clone()
                .unwrap_or_else(|| "pending".to_owned());
            inner.snapshot.stats.gateway = network.gateway.clone().unwrap_or_default();
            inner.snapshot.stats.mtu = if network.mtu > 0 {
                network.mtu as u32
            } else {
                inner.snapshot.stats.mtu
            };
            running.seed_network_meta(
                inner.snapshot.stats.assigned_ip.clone(),
                inner.snapshot.stats.gateway.clone(),
                inner.snapshot.stats.mtu,
            );
            inner.running_native = Some(running);
            inner.platform_vpn_running = true;
            inner.platform_vpn_starting = false;
            if inner.platform_start_outcome == PlatformStartOutcome::Pending {
                inner.platform_start_outcome = PlatformStartOutcome::Connected;
            }
            inner.connected_at = Some(Instant::now());
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Connected, None);
            self.push_diag_locked(
                &mut inner,
                "info",
                format!("OpenConnect mainloop attached to TUN fd {fd}"),
            );
            // Critical: the UI consumes this ashmem frame to leave
            // "establishing".
            self.persist_platform_locked(&mut inner)?;
            Ok(())
        }

        #[cfg(not(feature = "native-anyconnect"))]
        {
            let mut inner = self.lock()?;
            inner.platform_vpn_running = true;
            inner.platform_vpn_starting = false;
            if inner.platform_start_outcome == PlatformStartOutcome::Pending {
                inner.platform_start_outcome = PlatformStartOutcome::Connected;
            }
            inner.connected_at = Some(Instant::now());
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Connected, None);
            self.push_diag_locked(
                &mut inner,
                "info",
                format!("platform TUN fd {fd} accepted (no native OpenConnect)"),
            );
            self.persist_platform_locked(&mut inner)?;
            Ok(())
        }
    }

    pub async fn disconnect(&self) -> CoreResult<()> {
        self.disconnect_inner(None).await.map(|_| ())
    }

    /// Stop only the platform transaction currently owned by this process.
    ///
    /// VPN-extension cleanup is asynchronous. The UI may publish its next
    /// attempt while the old native worker is joining, so both the entry and
    /// completion sides must verify the captured owner before changing state.
    pub async fn disconnect_platform_attempt(&self, attempt_id: &str) -> CoreResult<bool> {
        if attempt_id.is_empty() {
            return Ok(false);
        }
        self.disconnect_inner(Some(attempt_id)).await
    }

    async fn disconnect_inner(&self, expected_attempt_id: Option<&str>) -> CoreResult<bool> {
        #[cfg(feature = "native-anyconnect")]
        let pending;
        #[cfg(feature = "native-anyconnect")]
        let running;
        let disconnect_generation = {
            let mut inner = self.lock()?;
            if expected_attempt_id
                .is_some_and(|attempt_id| inner.platform_start_attempt_id != attempt_id)
            {
                return Ok(false);
            }
            if matches!(inner.snapshot.lifecycle, ConnectionLifecycle::Disconnected) {
                return Ok(true);
            }
            // Unblock any auth worker waiting on a challenge form.
            self.auth.abort();
            inner.snapshot.pending_auth = None;
            if inner.platform_start_outcome == PlatformStartOutcome::Pending {
                inner.platform_start_outcome = PlatformStartOutcome::Cancelled;
            }
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Disconnecting, None);
            self.push_diag_locked(&mut inner, "info", "disconnect requested");
            inner.generation += 1;

            // Capture every native resource while the owner/generation check
            // is protected by the same lock. A concurrent bind can never make
            // this old cleanup take a newer attempt's worker.
            #[cfg(feature = "native-anyconnect")]
            {
                pending = inner.pending_native.take();
                running = inner.running_native.take();
            }
            inner.generation
        };

        #[cfg(feature = "native-anyconnect")]
        {
            drop(pending);
            if let Some(running) = running {
                running.cancel();
                let _ =
                    tokio::task::spawn_blocking(move || running.join(Duration::from_secs(8))).await;
            }
        }

        // Platform stop is asynchronous; mark disconnected if extension already gone.
        tokio::time::sleep(Duration::from_millis(200)).await;
        {
            let mut inner = self.lock()?;
            if inner.generation != disconnect_generation
                || expected_attempt_id
                    .is_some_and(|attempt_id| inner.platform_start_attempt_id != attempt_id)
            {
                return Ok(false);
            }
            if !inner.platform_vpn_running
                || matches!(
                    inner.snapshot.lifecycle,
                    ConnectionLifecycle::Disconnecting | ConnectionLifecycle::Connected
                )
            {
                inner.platform_vpn_running = false;
                inner.platform_vpn_starting = false;
                self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Disconnected, None);
                inner.connected_at = None;
                inner.snapshot.stats = SessionStats::default();
                inner.snapshot.network = NetworkSnapshot::default();
                inner.platform_session_handoff = None;
                inner.platform_browser_request = None;
                let _ = self.persist_platform_locked(&mut inner);
            }
        }
        Ok(true)
    }

    /// Called when the mainloop thread exits unexpectedly (peer hangup, error).
    pub fn on_native_session_ended(
        &self,
        session_generation: u64,
        error: Option<String>,
    ) -> CoreResult<()> {
        let mut inner = self.lock()?;
        if inner.generation != session_generation {
            return Ok(());
        }
        #[cfg(feature = "native-anyconnect")]
        {
            if let Some(running) = inner.running_native.take() {
                // Thread already finished; do not join again from here.
                drop(running);
            }
            inner.pending_native = None;
        }
        inner.platform_vpn_running = false;
        inner.platform_vpn_starting = false;
        let already_terminal = matches!(
            inner.snapshot.lifecycle,
            ConnectionLifecycle::Disconnected
                | ConnectionLifecycle::Disconnecting
                | ConnectionLifecycle::Failed
        ) || matches!(
            inner.platform_start_outcome,
            PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
        );
        if already_terminal {
            // A UI terminal verdict or explicit disconnect owns the outcome.
        } else if let Some(message) = error {
            inner.platform_start_outcome = PlatformStartOutcome::Failed;
            self.set_lifecycle_locked(
                &mut inner,
                ConnectionLifecycle::Failed,
                Some(message.clone()),
            );
            self.push_diag_locked(&mut inner, "error", message.clone());
        } else if !matches!(inner.snapshot.lifecycle, ConnectionLifecycle::Disconnected) {
            if inner.platform_start_outcome == PlatformStartOutcome::Pending {
                inner.platform_start_outcome = PlatformStartOutcome::Cancelled;
            }
            self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Disconnected, None);
        }
        inner.connected_at = None;
        self.persist_platform_locked(&mut inner)?;
        Ok(())
    }

    pub fn tick(&self) -> CoreResult<SessionSnapshot> {
        #[cfg(feature = "native-anyconnect")]
        let stop_native = {
            let mut inner = self.lock()?;
            // UI process: pick up Connected/Failed written by the VPN extension process.
            // Extension process: pick up a matching UI terminal verdict.
            self.sync_platform_locked(&mut inner);
            self.refresh_stats_locked(&mut inner);
            let is_extension = self
                .platform_ipc()?
                .is_some_and(|platform| !platform.is_ui());
            let owner_mismatch = inner.running_native.as_ref().is_some_and(|session| {
                session.attempt_id() != inner.platform_start_attempt_id
                    || session.generation() != inner.generation
            });
            let protect_error = inner
                .running_native
                .as_ref()
                .and_then(|session| session.take_protect_error().err());
            if let Some(error) = protect_error {
                let message = error.to_string();
                inner.generation = inner.generation.saturating_add(1);
                let session = inner.running_native.take();
                inner.pending_native = None;
                inner.platform_vpn_running = false;
                inner.platform_vpn_starting = false;
                inner.platform_start_outcome = PlatformStartOutcome::Failed;
                inner.connected_at = None;
                self.set_lifecycle_locked(
                    &mut inner,
                    ConnectionLifecycle::Failed,
                    Some(message.clone()),
                );
                self.push_diag_locked(&mut inner, "error", message);
                let _ = self.persist_platform_locked(&mut inner);
                session
            } else if is_extension
                && (owner_mismatch
                    || matches!(
                        inner.platform_start_outcome,
                        PlatformStartOutcome::Failed | PlatformStartOutcome::Cancelled
                    ))
            {
                let session = inner.running_native.take();
                if session.is_some() {
                    inner.pending_native = None;
                    inner.platform_vpn_running = false;
                    inner.platform_vpn_starting = false;
                    self.push_diag_locked(
                        &mut inner,
                        "warn",
                        "UI ended this attempt; stopping orphaned native session",
                    );
                    let _ = self.persist_platform_locked(&mut inner);
                }
                session
            } else {
                None
            }
        };
        #[cfg(not(feature = "native-anyconnect"))]
        {
            let mut inner = self.lock()?;
            self.sync_platform_locked(&mut inner);
            self.refresh_stats_locked(&mut inner);
        }

        #[cfg(feature = "native-anyconnect")]
        if let Some(session) = stop_native {
            session.cancel();
            let _ = session.join(Duration::from_secs(2));
        }

        let mut inner = self.lock()?;

        #[cfg(feature = "native-anyconnect")]
        {
            if let Some(running) = inner.running_native.as_ref() {
                if running.is_finished() {
                    // Mainloop exited; surface disconnect on next tick.
                    let finished = inner.running_native.take();
                    if let Some(finished) = finished {
                        match finished.join(Duration::from_millis(1)) {
                            Ok(()) => {
                                self.set_lifecycle_locked(
                                    &mut inner,
                                    ConnectionLifecycle::Disconnected,
                                    None,
                                );
                                inner.platform_vpn_running = false;
                                inner.connected_at = None;
                                let _ = self.persist_platform_locked(&mut inner);
                            }
                            Err(err) => {
                                let message = err.to_string();
                                self.set_lifecycle_locked(
                                    &mut inner,
                                    ConnectionLifecycle::Failed,
                                    Some(message.clone()),
                                );
                                inner.platform_vpn_running = false;
                                inner.connected_at = None;
                                let _ = self.persist_platform_locked(&mut inner);
                            }
                        }
                    }
                } else {
                    running.request_stats();
                    let mut traffic = running.traffic_snapshot();
                    traffic.assigned_ip = inner.snapshot.stats.assigned_ip.clone();
                    traffic.gateway = inner.snapshot.stats.gateway.clone();
                    traffic.mtu = inner.snapshot.stats.mtu;
                    if let Some(started) = inner.connected_at {
                        traffic.connected_seconds = started.elapsed().as_secs();
                    }
                    inner.snapshot.stats = traffic;
                    // Publish telemetry for the UI process.
                    let _ = self.persist_platform_locked(&mut inner);
                }
                return Ok(inner.snapshot.clone());
            }
        }

        if inner.dry_run && inner.snapshot.lifecycle.is_active() {
            // Dry-run / platform-only: synthetic traffic so the stats page moves.
            let stats = &mut inner.snapshot.stats;
            stats.bytes_sent = stats.bytes_sent.saturating_add(4096);
            stats.bytes_received = stats.bytes_received.saturating_add(16384);
            stats.packets_sent = stats.packets_sent.saturating_add(8);
            stats.packets_received = stats.packets_received.saturating_add(24);
        }
        Ok(inner.snapshot.clone())
    }

    pub fn subscribe_lifecycle(&self) -> watch::Receiver<ConnectionLifecycle> {
        self.lifecycle_tx.subscribe()
    }

    async fn prepare_dry_run(&self, profile: &ConnectionProfile) -> CoreResult<PreparedConnect> {
        tokio::time::sleep(Duration::from_millis(400)).await;
        if profile.server.contains("invalid") {
            return Err(CoreError::msg(
                "dry-run failure: server host contains 'invalid'",
            ));
        }
        let network = NetworkSnapshot {
            address: Some("10.64.12.48".to_owned()),
            netmask: Some("255.255.255.0".to_owned()),
            address_v6: None,
            netmask_v6: None,
            gateway: Some(profile.server.clone()),
            dns: vec!["1.1.1.1".to_owned()],
            mtu: if profile.mtu > 0 {
                profile.mtu as i32
            } else {
                1400
            },
            routes: Vec::new(),
            split_excludes: Vec::new(),
            domain: None,
            split_dns: Vec::new(),
        };
        let options = VpnOptions::from_network(&network, profile);
        Ok(PreparedConnect {
            network,
            options,
            #[cfg(feature = "native-anyconnect")]
            pending: None,
        })
    }

    async fn prepare_native(&self, profile: &ConnectionProfile) -> CoreResult<PreparedConnect> {
        #[cfg(feature = "native-anyconnect")]
        {
            {
                let mut inner = self.lock()?;
                self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Authenticating, None);
                self.push_diag_locked(
                    &mut inner,
                    "info",
                    "authenticate in UI process and hand off the resulting cookie",
                );
            }
            let profile = profile.clone();
            let interaction = Arc::clone(&self.auth);
            let pending = tokio::task::spawn_blocking(move || {
                native_authenticate(&profile, Some(interaction))
            })
            .await
            .map_err(|err| CoreError::msg(format!("authentication worker join: {err}")))??;
            let network = pending.network.clone();
            let options = pending.options.clone();
            Ok(PreparedConnect {
                network,
                options,
                pending: Some(pending),
            })
        }
        #[cfg(not(feature = "native-anyconnect"))]
        {
            let url = profile.server_url();
            if url.is_empty() {
                return Err(CoreError::msg("server address is empty"));
            }
            {
                let mut inner = self.lock()?;
                self.set_lifecycle_locked(&mut inner, ConnectionLifecycle::Authenticating, None);
            }
            tokio::time::sleep(Duration::from_millis(350)).await;
            if profile.server.contains("invalid") {
                return Err(CoreError::msg("server host rejected"));
            }
            let network = NetworkSnapshot {
                address: Some("10.64.12.48".to_owned()),
                netmask: Some("255.255.255.255".to_owned()),
                address_v6: None,
                netmask_v6: None,
                gateway: Some(profile.server.clone()),
                dns: vec!["1.1.1.1".to_owned(), "8.8.8.8".to_owned()],
                mtu: if profile.mtu > 0 {
                    profile.mtu as i32
                } else {
                    1400
                },
                routes: Vec::new(),
                split_excludes: Vec::new(),
                domain: None,
                split_dns: Vec::new(),
            };
            let options = VpnOptions::from_network(&network, profile);
            Ok(PreparedConnect { network, options })
        }
    }

    pub(super) fn refresh_stats_locked(&self, inner: &mut Inner) {
        if let Some(started) = inner.connected_at {
            if inner.snapshot.lifecycle.is_active() {
                inner.snapshot.stats.connected_seconds = started.elapsed().as_secs();
            }
        }
    }
}
