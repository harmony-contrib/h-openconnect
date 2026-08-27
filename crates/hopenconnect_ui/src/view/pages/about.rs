use super::super::*;

pub(crate) fn about_page(state: Signal<State>) -> Element {
    let current = state.read().clone();
    let locale = current.locale;
    let sdk_label = if current.snapshot.sdk_ready {
        translate_ui(locale, tr::sdk_ready())
    } else {
        translate_ui(locale, tr::sdk_pending())
    };
    let openconnect_version = current
        .snapshot
        .anyconnect_version
        .clone()
        .unwrap_or_else(|| translate_ui(locale, tr::sdk_pending()));

    let body = rsx! {
        column {
            width: "100%",
            column {
                width: "100%",
                padding: 20.0,
                background_color: surface(),
                border_width: 1.0,
                border_color: line(),
                border_radius: 16.0,
                align_items: "center",
                row {
                    width: 72.0,
                    height: 72.0,
                    align_items: "center",
                    justify_content: "center",
                    background_color: muted(),
                    border_radius: 18.0,
                    {arkit::icon("shield", 34.0, accent())}
                }
                text {
                    content: "H-OpenConnect",
                    margin_top: 14.0,
                    font_size: 22.0,
                    font_weight: 750,
                    font_color: text_color(),
                }
                text {
                    content: translate_ui(locale, tr::about_tagline()),
                    margin_top: 6.0,
                    font_size: 13.0,
                    font_color: subtle(),
                    text_align: "center",
                }
            }
            row { height: 14.0 }
            {settings_section(
                translate_ui(locale, tr::about_application()),
                vec![
                    settings_value_row("package", translate_ui(locale, tr::version()), current.snapshot.app_version.clone()),
                    settings_value_row("cpu", translate_ui(locale, tr::sdk_status()), sdk_label),
                    settings_value_row(
                        "layers",
                        "Backend",
                        current.snapshot.backend.clone(),
                    ),
                    settings_value_row(
                        "network",
                        "OpenConnect",
                        openconnect_version.clone(),
                    ),
                ],
            )}
            row { height: 14.0 }
            {settings_section(
                translate_ui(locale, tr::about_open_source()),
                vec![
                    open_source_row(
                        state,
                        "github",
                        "H-OpenConnect",
                        "MIT OR Apache-2.0",
                        "https://github.com/harmony-contrib/h-openconnect",
                    ),
                    open_source_row(
                        state,
                        "boxes",
                        "anyconnect-rs 0.1.1",
                        "MIT OR Apache-2.0",
                        "https://github.com/networks-rs/anyconnect-rs",
                    ),
                    open_source_row(
                        state,
                        "network",
                        format!("OpenConnect {openconnect_version}"),
                        "LGPL-2.1-only",
                        "https://gitlab.com/openconnect/openconnect/-/tree/8ae87c089bac597d9e09902bbedd03e0c45d8269",
                    ),
                    open_source_row(
                        state,
                        "layout-template",
                        "Arkit 75ff91c",
                        "MIT OR Apache-2.0",
                        "https://github.com/richerfu/arkit",
                    ),
                    open_source_row(
                        state,
                        "component",
                        "Dioxus 0.7.9",
                        "MIT OR Apache-2.0",
                        "https://github.com/DioxusLabs/dioxus",
                    ),
                ],
            )}
            row { height: 14.0 }
            {card(
                translate_ui(locale, tr::about_privacy()),
                None,
                rsx! {
                    column {
                        width: "100%",
                        {about_note_row(
                            "lock-keyhole",
                            translate_ui(locale, tr::about_privacy_storage()),
                        )}
                        {about_note_row(
                            "scroll-text",
                            translate_ui(locale, tr::about_privacy_logs()),
                        )}
                        {about_note_row(
                            "shield-check",
                            translate_ui(locale, tr::about_privacy_no_telemetry()),
                        )}
                        {about_note_row(
                            "fingerprint",
                            translate_ui(locale, tr::about_privacy_device_id()),
                        )}
                    }
                },
            )}
            text {
                content: translate_ui(locale, tr::about_disclaimer()),
                width: "100%",
                padding_top: 14.0,
                padding_right: 8.0,
                padding_bottom: 8.0,
                padding_left: 8.0,
                font_size: 11.0,
                line_height: 17.0,
                font_color: subtle(),
                text_align: "center",
            }
        }
    };

    scaffold(state, Route::About {}, rsx! {}, body)
}

fn open_source_row(
    state: Signal<State>,
    icon: &'static str,
    title: impl Into<String>,
    detail: impl Into<String>,
    url: &'static str,
) -> Element {
    let title = title.into();
    let detail = detail.into();
    rsx! {
        button {
            width: "100%",
            height: 68.0,
            padding: 0.0,
            background_color: surface(),
            border_width: 0.0,
            onclick: move |_| dispatch(state, Action::OpenExternalUrl(url.to_owned())),
            row {
                width: "100%",
                padding_right: 6.0,
                align_items: "center",
                row {
                    width: 36.0,
                    height: 36.0,
                    align_items: "center",
                    justify_content: "center",
                    background_color: muted(),
                    border_radius: 10.0,
                    {arkit::icon(icon, 16.0, text_color())}
                }
                column {
                    layout_weight: 1.0,
                    margin_left: 12.0,
                    align_items: "start",
                    text {
                        content: title,
                        font_size: 14.0,
                        font_weight: 650,
                        font_color: text_color(),
                    }
                    text {
                        content: detail,
                        margin_top: 2.0,
                        font_size: 11.0,
                        font_color: subtle(),
                        max_lines: 1_i32,
                        text_overflow: "ellipsis",
                    }
                }
                {arkit::icon("external-link", 16.0, subtle())}
            }
        }
    }
}

fn about_note_row(icon: &'static str, content: impl Into<String>) -> Element {
    let content = content.into();
    rsx! {
        row {
            width: "100%",
            margin_bottom: 10.0,
            align_items: "start",
            row {
                width: 20.0,
                height: 20.0,
                margin_top: 1.0,
                align_items: "center",
                justify_content: "center",
                {arkit::icon(icon, 15.0, success())}
            }
            row {
                layout_weight: 1.0,
                margin_left: 8.0,
                text {
                    content: content,
                    width: "100%",
                    font_size: 12.0,
                    line_height: 18.0,
                    font_color: text_color(),
                }
            }
        }
    }
}
