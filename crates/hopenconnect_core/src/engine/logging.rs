use super::*;

pub(super) fn install_runtime_log_layer() {
    INSTALL_RUNTIME_LOG_LAYER.call_once(|| {
        let subscriber = tracing_subscriber::registry().with(HOpenConnectLogLayer {
            logs: RUNTIME_LOGS.clone(),
        });
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}

struct HOpenConnectLogLayer {
    logs: Arc<Mutex<RuntimeLogBuffer>>,
}

impl<S> Layer<S> for HOpenConnectLogLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if !is_vpn_log_target(event.metadata().target()) {
            return;
        }
        let level = match *event.metadata().level() {
            Level::TRACE | Level::DEBUG => "debug",
            Level::INFO => "info",
            Level::WARN => "warning",
            Level::ERROR => "error",
        };
        let mut visitor = LogMessageVisitor::default();
        event.record(&mut visitor);
        if let Ok(mut logs) = self.logs.lock() {
            logs.capture(DiagnosticEntry {
                level: level.to_owned(),
                message: visitor.finish(event.metadata().target()),
                timestamp: now_timestamp(),
            });
        }
    }
}

fn is_vpn_log_target(target: &str) -> bool {
    target.starts_with("hopenconnect_core")
        || target.starts_with("anyconnect")
        || target.starts_with("openconnect")
}

#[derive(Default)]
struct LogMessageVisitor {
    message: Option<String>,
    fields: Vec<String>,
}

impl LogMessageVisitor {
    fn finish(self, fallback: &str) -> String {
        let mut message = self.message.unwrap_or_else(|| fallback.to_owned());
        if !self.fields.is_empty() {
            message.push_str(" · ");
            message.push_str(&self.fields.join(", "));
        }
        message
    }
}

impl tracing::field::Visit for LogMessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let value = format!("{value:?}");
        if field.name() == "message" {
            self.message = Some(value.trim_matches('"').to_owned());
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_owned());
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }
}

pub(super) fn merged_logs(state_logs: &[DiagnosticEntry]) -> Vec<DiagnosticEntry> {
    let state_start = state_logs.len().saturating_sub(MAX_IN_MEMORY_LOGS);
    let mut logs = state_logs[state_start..].to_vec();
    let remaining = MAX_IN_MEMORY_LOGS.saturating_sub(logs.len());
    if let Ok(runtime_logs) = RUNTIME_LOGS.lock() {
        let runtime_start = runtime_logs.len().saturating_sub(remaining);
        logs.extend(runtime_logs.entries().skip(runtime_start).cloned());
    }
    logs
}

pub(super) fn merge_platform_logs(
    mut local: Vec<DiagnosticEntry>,
    platform: &[DiagnosticEntry],
) -> Vec<DiagnosticEntry> {
    for entry in platform {
        if !local.iter().any(|existing| {
            existing.level == entry.level
                && existing.message == entry.message
                && existing.timestamp == entry.timestamp
        }) {
            local.push(entry.clone());
        }
    }
    if local.len() > MAX_IN_MEMORY_LOGS {
        local.drain(..local.len() - MAX_IN_MEMORY_LOGS);
    }
    local
}
