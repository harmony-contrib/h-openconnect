use super::super::*;

pub(crate) fn home_page(state: Signal<State>) -> Element {
    let current = state.read().clone();
    let lifecycle = current.snapshot.lifecycle;
    let busy = lifecycle.is_busy();
    let active = lifecycle.is_active();
    let status_color = lifecycle_color(lifecycle);
    let status_label = lifecycle_label(current.locale, lifecycle);
    let connection = current.active_connection().cloned();
    let connection_name = connection
        .as_ref()
        .map(|item| item.name.clone())
        .unwrap_or_else(|| translate_ui(current.locale, tr::no_connection()));
    let server = connection
        .as_ref()
        .map(|item| item.server.clone())
        .unwrap_or_else(|| "—".to_owned());
    let group = connection
        .as_ref()
        .map(|item| {
            if item.group.is_empty() {
                "—".to_owned()
            } else {
                item.group.clone()
            }
        })
        .unwrap_or_else(|| "—".to_owned());
    let protocol = connection
        .as_ref()
        .map(|item| item.protocol.as_label().to_owned())
        .unwrap_or_else(|| "—".to_owned());
    let action_label = if active {
        translate_ui(current.locale, tr::disconnect())
    } else if matches!(
        lifecycle,
        ConnectionLifecycle::Connecting
            | ConnectionLifecycle::Authenticating
            | ConnectionLifecycle::Establishing
    ) {
        translate_ui(current.locale, tr::connecting())
    } else if matches!(lifecycle, ConnectionLifecycle::Disconnecting) {
        translate_ui(current.locale, tr::disconnecting())
    } else {
        translate_ui(current.locale, tr::connect())
    };
    let action_icon = if active { "square" } else { "power" };
    let stats = current.snapshot.stats.clone();
    let navigator = use_navigator();

    let body = rsx! {
        column {
            width: "100%",
            // Brand / status hero
            column {
                width: "100%",
                padding: 20.0,
                background_color: surface(),
                border_width: 1.0,
                border_color: line(),
                border_radius: 16.0,
                align_items: "center",
                row {
                    width: 88.0,
                    height: 88.0,
                    align_items: "center",
                    justify_content: "center",
                    background_color: muted(),
                    border_radius: 44.0,
                    border_width: 2.0,
                    border_color: status_color,
                    if busy {
                        Spinner { size: 34.0, color: Some(status_color) }
                    } else {
                        {arkit::icon(if active { "shield-check" } else { "shield" }, 36.0, status_color)}
                    }
                }
                text {
                    content: translate_ui(current.locale, tr::home_title()),
                    margin_top: 16.0,
                    font_size: 13.0,
                    font_weight: 600,
                    font_color: subtle(),
                }
                text {
                    content: status_label,
                    margin_top: 4.0,
                    font_size: 26.0,
                    font_weight: 750,
                    font_color: status_color,
                }
                text {
                    content: connection_name.clone(),
                    margin_top: 6.0,
                    font_size: 15.0,
                    font_weight: 650,
                    font_color: text_color(),
                }
                if let Some(error) = current.snapshot.last_error.clone() {
                    text {
                        content: sanitize_display_text(&error),
                        margin_top: 8.0,
                        font_size: 12.0,
                        font_color: danger(),
                        text_align: "center",
                    }
                }
                row { height: 18.0 }
                FlatButton {
                    variant: if active { FlatButtonVariant::Destructive } else { FlatButtonVariant::Accent },
                    size: ButtonSize::Lg,
                    width: Some("100%".to_owned()),
                    disabled: Some(busy || connection.is_none()),
                    onclick: move |_| dispatch(state, Action::ToggleConnect),
                    if busy {
                        Spinner { size: 18.0, color: Some(0xFFFFFFFF) }
                        text { content: action_label, margin_left: 8.0, font_size: 16.0, font_weight: 700, font_color: 0xFFFFFFFFu32 }
                    } else {
                        {arkit::icon(action_icon, 18.0, 0xFFFFFFFF)}
                        text { content: action_label, margin_left: 8.0, font_size: 16.0, font_weight: 700, font_color: 0xFFFFFFFFu32 }
                    }
                }
            }
            row { height: 12.0 }
            {card(
                translate_ui(current.locale, tr::select_connection()),
                Some(if current.snapshot.sdk_ready {
                    translate_ui(current.locale, tr::home_live_session())
                } else {
                    translate_ui(current.locale, tr::home_not_linked())
                }),
                rsx! {
                    column {
                        width: "100%",
                        row {
                            width: "100%",
                            align_items: "center",
                            column {
                                layout_weight: 1.0,
                                align_items: "start",
                                text { content: translate_ui(current.locale, tr::server()), font_size: 12.0, font_color: subtle() }
                                text { content: server.clone(), margin_top: 2.0, font_size: 15.0, font_weight: 650, font_color: text_color() }
                            }
                            Badge {
                                content: protocol.clone(),
                                variant: BadgeVariant::Secondary,
                            }
                        }
                        row { height: 12.0 }
                        Separator {}
                        row { height: 12.0 }
                        row {
                            width: "100%",
                            column {
                                layout_weight: 1.0,
                                align_items: "start",
                                text { content: translate_ui(current.locale, tr::group()), font_size: 12.0, font_color: subtle() }
                                text { content: group, margin_top: 2.0, font_size: 14.0, font_weight: 600, font_color: text_color() }
                            }
                            FlatButton {
                                variant: FlatButtonVariant::Outline,
                                size: ButtonSize::Sm,
                                onclick: move |_| {
                                    navigator.push(Route::Connections {});
                                },
                                text { content: translate_ui(current.locale, tr::nav_connections()), font_size: 13.0, font_weight: 600, font_color: text_color() }
                            }
                        }
                    }
                }
            )}
            if active {
                row { height: 12.0 }
                row {
                    width: "100%",
                    {metric_tile("network", translate_ui(current.locale, tr::assigned_ip()), if stats.assigned_ip.is_empty() { "—".to_owned() } else { stats.assigned_ip.clone() })}
                    row { width: 10.0 }
                    {metric_tile("clock", translate_ui(current.locale, tr::duration()), format_duration(stats.connected_seconds))}
                }
                row { height: 10.0 }
                row {
                    width: "100%",
                    {metric_tile("arrow-up", translate_ui(current.locale, tr::sent()), format_bytes(stats.bytes_sent))}
                    row { width: 10.0 }
                    {metric_tile("arrow-down", translate_ui(current.locale, tr::received()), format_bytes(stats.bytes_received))}
                }
            }
        }
    };

    scaffold(state, Route::Home {}, rsx! {}, body)
}
