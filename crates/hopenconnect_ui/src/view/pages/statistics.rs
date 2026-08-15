use super::super::*;

pub(crate) fn statistics_page(state: Signal<State>) -> Element {
    let current = state.read().clone();
    let lifecycle = current.snapshot.lifecycle;
    let stats = current.snapshot.stats.clone();
    let active = lifecycle.is_active();
    let connection_name = current
        .active_connection()
        .map(|item| item.name.clone())
        .unwrap_or_else(|| translate_ui(current.locale, tr::no_connection()));

    let body = rsx! {
        column {
            width: "100%",
            {card(
                lifecycle_label(current.locale, lifecycle),
                Some(connection_name),
                rsx! {
                    column {
                        width: "100%",
                        if active {
                            text {
                                content: format!("{}  {}", translate_ui(current.locale, tr::duration()), format_duration(stats.connected_seconds)),
                                font_size: 28.0,
                                font_weight: 750,
                                font_color: text_color(),
                            }
                            text {
                                content: format!("{} · {}", translate_ui(current.locale, tr::gateway()), if stats.gateway.is_empty() { "—" } else { &stats.gateway }),
                                margin_top: 6.0,
                                font_size: 13.0,
                                font_color: subtle(),
                            }
                        } else {
                            text {
                                content: translate_ui(current.locale, tr::disconnected()),
                                font_size: 18.0,
                                font_weight: 700,
                                font_color: subtle(),
                            }
                            text {
                                content: translate_ui(current.locale, tr::statistics_hint()),
                                margin_top: 8.0,
                                font_size: 12.0,
                                font_color: subtle(),
                            }
                        }
                    }
                }
            )}
            row { height: 12.0 }
            row {
                width: "100%",
                {metric_tile("arrow-up", translate_ui(current.locale, tr::sent()), format_bytes(stats.bytes_sent))}
                row { width: 10.0 }
                {metric_tile("arrow-down", translate_ui(current.locale, tr::received()), format_bytes(stats.bytes_received))}
            }
            row { height: 10.0 }
            row {
                width: "100%",
                {metric_tile("send", translate_ui(current.locale, tr::packets_sent()), stats.packets_sent.to_string())}
                row { width: 10.0 }
                {metric_tile("inbox", translate_ui(current.locale, tr::packets_received()), stats.packets_received.to_string())}
            }
            row { height: 10.0 }
            row {
                width: "100%",
                {metric_tile("network", translate_ui(current.locale, tr::assigned_ip()), if stats.assigned_ip.is_empty() { "—".to_owned() } else { stats.assigned_ip })}
                row { width: 10.0 }
                {metric_tile("gauge", translate_ui(current.locale, tr::mtu()), if stats.mtu == 0 { "—".to_owned() } else { stats.mtu.to_string() })}
            }
        }
    };

    scaffold(state, Route::Statistics {}, rsx! {}, body)
}
