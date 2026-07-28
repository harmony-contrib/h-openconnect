use super::super::*;

pub(crate) fn more_page(state: Signal<State>) -> Element {
    let current = state.read().clone();
    let s = strings(current.locale);
    let body = rsx! {
        column {
            width: "100%",
            {settings_section(
                tr(current.locale, "偏好", "Preferences"),
                vec![
                    settings_route_row(
                        Route::Appearance {},
                        tr(current.locale, "语言与浅色 / 深色主题", "Language and light / dark theme"),
                    ),
                ],
            )}
            row { height: 16.0 }
            {settings_section(
                tr(current.locale, "运维", "Operations"),
                vec![
                    settings_route_row(
                        Route::Diagnostics {},
                        tr(current.locale, "查看连接诊断日志", "Connection diagnostics"),
                    ),
                    settings_route_row(
                        Route::About {},
                        tr(current.locale, "版本与 OpenConnect 状态", "Version and OpenConnect status"),
                    ),
                ],
            )}
        }
    };
    scaffold(state, Route::More {}, rsx! {}, body)
}

pub(crate) fn appearance_page(state: Signal<State>) -> Element {
    let current = state.read().clone();
    let locale = current.locale;
    let s = strings(locale);

    let system_language = s.system.to_owned();
    let simplified_chinese = "简体中文".to_owned();
    let english = "English".to_owned();
    let selected_language = match current.language_preference {
        LanguagePreference::System => system_language.clone(),
        LanguagePreference::ZhCn => simplified_chinese.clone(),
        LanguagePreference::En => english.clone(),
    };
    let language_system_option = system_language.clone();
    let language_chinese_option = simplified_chinese.clone();

    let system_theme = s.system.to_owned();
    let light_theme = s.light.to_owned();
    let dark_theme = s.dark.to_owned();
    let selected_theme = match current.theme_preference {
        ThemePreference::System => system_theme.clone(),
        ThemePreference::Light => light_theme.clone(),
        ThemePreference::Dark => dark_theme.clone(),
    };
    let theme_system_option = system_theme.clone();
    let theme_light_option = light_theme.clone();

    let body = rsx! {
        column {
            width: "100%",
            {card(
                s.language,
                Some(tr(locale, "选择界面语言；跟随系统会响应系统语言变化", "Choose the interface language; System follows device changes").to_owned()),
                rsx! {
                    RadioGroup {
                        options: vec![system_language, simplified_chinese, english],
                        selected: Some(selected_language),
                        on_select: move |value: String| {
                            let preference = if value == language_system_option {
                                LanguagePreference::System
                            } else if value == language_chinese_option {
                                LanguagePreference::ZhCn
                            } else {
                                LanguagePreference::En
                            };
                            dispatch(state, Action::SetLanguagePreference(preference));
                        }
                    }
                }
            )}
            row { height: 12.0 }
            {card(
                s.theme,
                Some(tr(locale, "切换浅色、深色或跟随系统；修改会立即生效", "Use light, dark, or the system appearance; changes apply immediately").to_owned()),
                rsx! {
                    RadioGroup {
                        options: vec![system_theme, light_theme, dark_theme],
                        selected: Some(selected_theme),
                        on_select: move |value: String| {
                            let preference = if value == theme_system_option {
                                ThemePreference::System
                            } else if value == theme_light_option {
                                ThemePreference::Light
                            } else {
                                ThemePreference::Dark
                            };
                            dispatch(state, Action::SetThemePreference(preference));
                        }
                    }
                }
            )}
        }
    };

    scaffold(state, Route::Appearance {}, rsx! {}, body)
}

pub(crate) fn diagnostics_page(state: Signal<State>) -> Element {
    let current = state.read().clone();
    let s = strings(current.locale);
    let entries = current.snapshot.diagnostics.clone();

    let actions = rsx! {
        FlatButton {
            variant: FlatButtonVariant::Ghost,
            size: ButtonSize::Sm,
            onclick: move |_| dispatch(state, Action::ClearDiagnostics),
            text { content: s.clear_logs, font_size: 13.0, font_weight: 600, font_color: text_color() }
        }
    };

    let body = rsx! {
        column {
            width: "100%",
            align_items: "stretch",
            if entries.is_empty() {
                column {
                    width: "100%",
                    padding: 24.0,
                    align_items: "start",
                    background_color: surface(),
                    border_width: 1.0,
                    border_color: line(),
                    border_radius: 12.0,
                    text {
                        content: s.no_logs,
                        width: "100%",
                        font_size: 14.0,
                        font_color: subtle(),
                        text_align: "start",
                    }
                }
            } else {
                {entries.into_iter().map(|entry| {
                    let level_color = match entry.level.as_str() {
                        "error" => danger(),
                        "warn" => warning(),
                        _ => accent(),
                    };
                    rsx! {
                        column {
                            width: "100%",
                            margin_bottom: 8.0,
                            padding: 12.0,
                            align_items: "stretch",
                            background_color: surface(),
                            border_width: 1.0,
                            border_color: line(),
                            border_radius: 10.0,
                            // Left accent strip for level (error / warn / info).
                            row {
                                width: "100%",
                                height: 3.0,
                                margin_bottom: 8.0,
                                background_color: level_color,
                                border_radius: 2.0,
                            }
                            row {
                                width: "100%",
                                align_items: "center",
                                justify_content: "start",
                                Badge {
                                    content: entry.level.to_uppercase(),
                                    variant: BadgeVariant::Secondary,
                                }
                                row { layout_weight: 1.0 }
                                text {
                                    content: entry.timestamp,
                                    font_size: 11.0,
                                    font_color: subtle(),
                                    text_align: "end",
                                }
                            }
                            text {
                                content: entry.message,
                                width: "100%",
                                margin_top: 8.0,
                                font_size: 13.0,
                                font_color: text_color(),
                                text_align: "start",
                            }
                        }
                    }
                })}
            }
        }
    };

    scaffold(state, Route::Diagnostics {}, actions, body)
}

pub(crate) fn about_page(state: Signal<State>) -> Element {
    let current = state.read().clone();
    let s = strings(current.locale);
    let sdk_label = if current.snapshot.sdk_ready {
        s.sdk_ready
    } else {
        s.sdk_pending
    };

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
                    content: "H-AnyConnect",
                    margin_top: 14.0,
                    font_size: 22.0,
                    font_weight: 750,
                    font_color: text_color(),
                }
                text {
                    content: tr(current.locale, "HarmonyOS 安全远程接入客户端", "Secure remote access client for HarmonyOS"),
                    margin_top: 6.0,
                    font_size: 13.0,
                    font_color: subtle(),
                    text_align: "center",
                }
            }
            row { height: 14.0 }
            {settings_section(
                tr(current.locale, "应用信息", "Application"),
                vec![
                    settings_value_row("package", s.version, current.snapshot.app_version.clone()),
                    settings_value_row("cpu", s.sdk_status, sdk_label),
                    settings_value_row(
                        "layers",
                        "Backend",
                        current.snapshot.backend.clone(),
                    ),
                    settings_value_row(
                        "network",
                        "OpenConnect",
                        current
                            .snapshot
                            .anyconnect_version
                            .clone()
                            .unwrap_or_else(|| "not linked".to_owned()),
                    ),
                ],
            )}
        }
    };

    scaffold(state, Route::About {}, rsx! {}, body)
}
