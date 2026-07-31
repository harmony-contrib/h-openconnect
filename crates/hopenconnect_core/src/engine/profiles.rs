use super::*;

impl SessionEngine {
    pub fn configure_home(&self, home: impl Into<PathBuf>) -> CoreResult<()> {
        let home = home.into();
        remove_obsolete_cross_process_files(&home)?;
        let store = ProfileStore::open(&home)?;
        // Production starts empty. Demo/test profiles must be created by test
        // fixtures instead of leaking into the real application.
        let profiles = store.load()?;
        if !store.profiles_file_exists() {
            store.save(&profiles)?;
        }
        let preferences = store.load_preferences().unwrap_or_default();
        let mut inner = self.lock()?;
        inner.home = home.clone();
        inner.logs = RecordedLogBuffer::new(home);
        inner.platform_diagnostics.clear();
        inner.store = Some(store);
        inner.preferences = preferences.clone();
        inner.snapshot.connections = profiles;
        // Prefer persisted selection; fall back to first profile; never keep a
        // stale seed id (e.g. "demo-hq") that is not in the loaded list.
        let resolved_active = preferences
            .active_connection_id
            .filter(|id| inner.snapshot.connections.iter().any(|p| p.id == *id))
            .or_else(|| {
                inner
                    .snapshot
                    .active_connection_id
                    .clone()
                    .filter(|id| inner.snapshot.connections.iter().any(|p| p.id == *id))
            })
            .or_else(|| inner.snapshot.connections.first().map(|p| p.id.clone()));
        inner.snapshot.active_connection_id = resolved_active;
        self.persist_preferences_locked(&inner)?;
        // Absorb a sibling-process frame when this process has already
        // attached to the current ashmem session.
        self.sync_platform_locked(&mut inner);
        Ok(())
    }

    pub fn snapshot(&self) -> CoreResult<SessionSnapshot> {
        let mut inner = self.lock()?;
        self.sync_platform_locked(&mut inner);
        self.refresh_stats_locked(&mut inner);
        inner.logs.sync_session();
        if let Ok(mut runtime_logs) = RUNTIME_LOGS.lock() {
            runtime_logs.sync(inner.logs.root());
        }
        inner.snapshot.diagnostics = if inner.logs.enabled() {
            merge_platform_logs(merged_logs(&inner.logs), &inner.platform_diagnostics)
        } else {
            Vec::new()
        };
        inner.snapshot.pending_auth = self.auth.pending();
        Ok(inner.snapshot.clone())
    }

    pub fn appearance_preferences(&self) -> CoreResult<(String, String)> {
        let inner = self.lock()?;
        Ok((
            inner.preferences.language.clone(),
            inner.preferences.theme.clone(),
        ))
    }

    pub fn set_appearance_preferences(&self, language: &str, theme: &str) -> CoreResult<()> {
        if !matches!(language, "system" | "zh-CN" | "en") {
            return Err(CoreError::msg("unsupported language preference"));
        }
        if !matches!(theme, "system" | "light" | "dark") {
            return Err(CoreError::msg("unsupported theme preference"));
        }
        let mut inner = self.lock()?;
        inner.preferences.language = language.to_owned();
        inner.preferences.theme = theme.to_owned();
        self.persist_preferences_locked(&inner)
    }

    /// Pending interactive auth form, if the OpenConnect worker is blocked on UI.
    pub fn pending_auth(&self) -> Option<AuthChallenge> {
        self.auth.pending()
    }

    /// Submit field values for the current challenge (unblocks the auth worker).
    pub fn submit_auth_challenge(&self, reply: AuthChallengeReply) -> CoreResult<()> {
        self.auth.submit(reply)?;
        Ok(())
    }

    /// Cancel the current challenge (and the in-flight connect).
    pub fn cancel_auth_challenge(&self) -> CoreResult<()> {
        self.auth.abort();
        if let Ok(mut inner) = self.lock() {
            inner.snapshot.pending_auth = None;
        }
        Ok(())
    }

    pub fn snapshot_json(&self) -> CoreResult<String> {
        Ok(serde_json::to_string(&self.snapshot()?)?)
    }

    pub fn clear_diagnostics(&self) -> CoreResult<()> {
        let mut inner = self.lock()?;
        inner.logs.clear();
        inner.platform_diagnostics.clear();
        inner.snapshot.diagnostics.clear();
        if let Ok(mut logs) = RUNTIME_LOGS.lock() {
            logs.clear();
        }
        for name in [
            "openconnect-progress.log",
            "openconnect-progress.log.1",
            "last-connect-error.txt",
        ] {
            let _ = std::fs::remove_file(inner.home.join(name));
        }
        Ok(())
    }

    pub fn log_recording_status(&self) -> CoreResult<crate::LogRecordingStatus> {
        let inner = self.lock()?;
        log_recording::recording_status(&inner.home)
    }

    pub fn set_log_recording_enabled(
        &self,
        enabled: bool,
    ) -> CoreResult<crate::LogRecordingStatus> {
        let mut inner = self.lock()?;
        let root = inner.home.clone();
        let was_enabled = log_recording::recording_status(&root)?.enabled;
        if was_enabled == enabled {
            return log_recording::recording_status(&root);
        }

        if enabled {
            inner.logs.clear();
            inner.platform_diagnostics.clear();
            inner.snapshot.diagnostics.clear();
            if let Ok(mut logs) = RUNTIME_LOGS.lock() {
                logs.clear();
            }
            log_recording::set_recording_enabled(&root, true)?;
            inner.logs.sync_session();
            self.push_diag_locked(&mut inner, "info", "log recording enabled");
        } else {
            self.push_diag_locked(&mut inner, "info", "log recording disabled");
            log_recording::set_recording_enabled(&root, false)?;
            inner.logs.sync_session();
            inner.platform_diagnostics.clear();
            inner.snapshot.diagnostics.clear();
            if let Ok(mut logs) = RUNTIME_LOGS.lock() {
                logs.clear();
            }
        }
        log_recording::recording_status(&root)
    }

    pub fn read_log_archive(&self, file_name: &str) -> CoreResult<String> {
        let inner = self.lock()?;
        log_recording::read_archive(&inner.home, file_name)
    }

    pub fn delete_log_archive(&self, file_name: &str) -> CoreResult<crate::LogRecordingStatus> {
        let inner = self.lock()?;
        log_recording::delete_archive(&inner.home, file_name)?;
        log_recording::recording_status(&inner.home)
    }

    pub fn set_profiles(&self, profiles: Vec<ConnectionProfile>) -> CoreResult<()> {
        let mut inner = self.lock()?;
        if let Some(store) = &inner.store {
            store.save(&profiles)?;
        }
        inner.snapshot.connections = profiles;
        Ok(())
    }

    pub fn upsert_profile(&self, profile: ConnectionProfile) -> CoreResult<()> {
        profile.validate().map_err(CoreError::msg)?;
        let mut inner = self.lock()?;
        let previous = inner
            .snapshot
            .connections
            .iter()
            .find(|item| item.id == profile.id)
            .cloned();
        let edits_live_profile = inner.snapshot.active_connection_id.as_deref()
            == Some(profile.id.as_str())
            && (inner.snapshot.lifecycle.is_busy() || inner.snapshot.lifecycle.is_active());
        if edits_live_profile {
            return Err(CoreError::msg(
                "disconnect before editing the active connection",
            ));
        }
        if let Some(existing) = inner
            .snapshot
            .connections
            .iter_mut()
            .find(|item| item.id == profile.id)
        {
            *existing = profile;
        } else {
            // New profiles become active unless another profile owns a live session.
            let new_id = profile.id.clone();
            inner.snapshot.connections.insert(0, profile);
            if !inner.snapshot.lifecycle.is_busy() && !inner.snapshot.lifecycle.is_active() {
                inner.snapshot.active_connection_id = Some(new_id);
            }
        }
        if let Some(store) = &inner.store {
            store.save(&inner.snapshot.connections)?;
        }
        self.persist_preferences_locked(&inner)?;
        if let Some(previous) = previous {
            cleanup_unreferenced_profile_files(&inner, &previous);
        }
        Ok(())
    }

    /// Toggle full-tunnel routing on the active profile.
    ///
    /// Persists immediately. When a session is up, also rewrites
    /// `last_vpn_options` / session handoff so a reconnect (or extension
    /// restart) picks up `0.0.0.0/0` without re-auth for credentials.
    /// System VPN routes only change after the extension recreates the TUN.
    pub fn set_active_force_global(&self, force_global: bool) -> CoreResult<bool> {
        let mut inner = self.lock()?;
        let id = inner
            .snapshot
            .active_connection_id
            .clone()
            .ok_or_else(|| CoreError::msg("no active connection"))?;
        let profile = inner
            .snapshot
            .connections
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| CoreError::msg(format!("unknown profile {id}")))?;
        profile.force_global = force_global;
        if let Some(store) = &inner.store {
            store.save(&inner.snapshot.connections)?;
        }
        self.persist_preferences_locked(&inner)?;

        let session_live = inner.snapshot.lifecycle.is_active()
            || inner.platform_vpn_running
            || inner.platform_vpn_starting;
        if session_live {
            // Preserve cookie / credentials while rewriting route policy.
            let cookie = inner.last_vpn_options.cookie.clone();
            let server = inner.last_vpn_options.server.clone();
            let username = inner.last_vpn_options.username.clone();
            let password = inner.last_vpn_options.password.clone();
            let group = inner.last_vpn_options.group.clone();
            let accept_untrusted = inner.last_vpn_options.accept_untrusted;
            let profile = inner
                .snapshot
                .connections
                .iter()
                .find(|item| item.id == id)
                .cloned();
            if let Some(profile) = profile {
                let mut rebuilt = VpnOptions::from_network(&inner.snapshot.network, &profile);
                // Keep addresses/DNS/MTU from the live session when the network
                // snapshot is incomplete (common right after cookie resume).
                if rebuilt.addresses.is_empty() {
                    rebuilt.addresses = inner.last_vpn_options.addresses.clone();
                }
                if rebuilt.dns_addresses.is_empty() {
                    rebuilt.dns_addresses = inner.last_vpn_options.dns_addresses.clone();
                }
                if rebuilt.mtu == 0 {
                    rebuilt.mtu = inner.last_vpn_options.mtu;
                }
                rebuilt.cookie = cookie;
                rebuilt.server = server;
                rebuilt.username = username;
                rebuilt.password = password;
                rebuilt.group = group;
                rebuilt.accept_untrusted = accept_untrusted;
                rebuilt.force_global = force_global;
                rebuilt.apply_force_global();
                inner.last_vpn_options = rebuilt;
            } else {
                inner.last_vpn_options.force_global = force_global;
                inner.last_vpn_options.apply_force_global();
            }
            let handoff = SessionHandoff {
                attempt_id: inner.platform_start_attempt_id.clone(),
                options: inner.last_vpn_options.clone(),
                network: inner.snapshot.network.clone(),
                updated_at: PlatformVpnState::now_nanos(),
            };
            inner.platform_session_handoff = Some(handoff);
            let _ = self.persist_platform_locked(&mut inner);
            let first_route = inner.last_vpn_options.routes.first().cloned();
            self.push_diag_locked(
                &mut inner,
                "info",
                format!("force_global={force_global} routes={first_route:?}"),
            );
        }
        Ok(session_live)
    }

    pub fn delete_profile(&self, id: &str) -> CoreResult<()> {
        let mut inner = self.lock()?;
        if inner.snapshot.active_connection_id.as_deref() == Some(id)
            && (inner.snapshot.lifecycle.is_busy() || inner.snapshot.lifecycle.is_active())
        {
            return Err(CoreError::msg(
                "disconnect before deleting the active connection",
            ));
        }
        let removed = inner
            .snapshot
            .connections
            .iter()
            .find(|item| item.id == id)
            .cloned();
        let Some(removed) = removed else {
            // Idempotent: UI may re-fire; never panic on a missing id.
            return Ok(());
        };
        inner.snapshot.connections.retain(|item| item.id != id);
        if inner.snapshot.active_connection_id.as_deref() == Some(id) {
            inner.snapshot.active_connection_id =
                inner.snapshot.connections.first().map(|p| p.id.clone());
        }
        if let Some(store) = &inner.store {
            store.save(&inner.snapshot.connections)?;
        }
        self.persist_preferences_locked(&inner)?;
        cleanup_unreferenced_profile_files(&inner, &removed);
        Ok(())
    }

    pub fn select_profile(&self, id: &str) -> CoreResult<()> {
        let mut inner = self.lock()?;
        if !inner.snapshot.connections.iter().any(|item| item.id == id) {
            return Err(CoreError::msg(format!("unknown profile {id}")));
        }
        if inner.snapshot.lifecycle.is_busy() || inner.snapshot.lifecycle.is_active() {
            return Err(CoreError::msg(
                "disconnect before changing the active profile",
            ));
        }
        inner.snapshot.active_connection_id = Some(id.to_owned());
        self.persist_preferences_locked(&inner)?;
        Ok(())
    }

    pub fn active_profile(&self) -> CoreResult<Option<ConnectionProfile>> {
        let inner = self.lock()?;
        let id = inner.snapshot.active_connection_id.clone();
        Ok(id.and_then(|id| {
            inner
                .snapshot
                .connections
                .iter()
                .find(|item| item.id == id)
                .cloned()
        }))
    }

    /// Read the initial authentication form for a server so the editor can
    /// present the exact group values advertised by the headend.
    pub async fn discover_auth_groups(
        &self,
        profile: ConnectionProfile,
    ) -> CoreResult<AuthGroupDiscovery> {
        #[cfg(feature = "native-anyconnect")]
        {
            tokio::task::spawn_blocking(move || {
                crate::native_session::discover_auth_groups(&profile)
            })
            .await
            .map_err(|err| CoreError::msg(format!("group discovery task failed: {err}")))?
        }
        #[cfg(not(feature = "native-anyconnect"))]
        {
            let _ = profile;
            Err(CoreError::msg(
                "authentication group discovery requires native AnyConnect support",
            ))
        }
    }

    pub fn default_vpn_options_json(&self) -> CoreResult<String> {
        let inner = self.lock()?;
        Ok(serde_json::to_string(&inner.last_vpn_options)?)
    }
}
