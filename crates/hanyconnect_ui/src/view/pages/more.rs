use super::super::*;

pub(crate) fn more_page(state: Signal<State>) -> Element {
    let current = state.read().clone();
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
    let mut pending_delete = use_signal(|| None::<String>);
    let current = state.read().clone();
    let locale = current.locale;
    let entries = current
        .snapshot
        .diagnostics
        .clone()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    let recording_enabled = current.log_recording.enabled;
    let recording_pending = current.log_recording_pending;
    let export_pending = current.log_archive_export_pending.clone();
    let delete_pending = current.log_archive_delete_pending.clone();
    let selected_delete = pending_delete();
    let archives = current.log_recording.archives.clone();
    let archive_count = archives.len();

    let actions = rsx! {
        FlatButton {
            variant: FlatButtonVariant::Ghost,
            size: ButtonSize::Icon,
            disabled: Some(recording_pending),
            onclick: move |_| dispatch(state, Action::ToggleLogRecording),
            if recording_pending {
                Spinner { size: 17.0, color: Some(text_color()) }
            } else if recording_enabled {
                {arkit::icon("square", 17.0, danger())}
            } else {
                {arkit::icon("play", 17.0, success())}
            }
        }
    };

    let body = rsx! {
        column {
            width: "100%",
            align_items: "stretch",
            column {
                width: "100%",
                padding: 14.0,
                background_color: surface(),
                border_width: 1.0,
                border_color: line(),
                border_radius: 12.0,
                row {
                    width: "100%",
                    align_items: "center",
                    text {
                        content: if recording_enabled {
                            tr(locale, "正在记录并按天保存", "Recording and saving daily")
                        } else {
                            tr(locale, "日志记录已关闭", "Log recording is off")
                        },
                        font_size: 13.0,
                        font_weight: 650,
                        font_color: if recording_enabled { success() } else { subtle() },
                    }
                    row { layout_weight: 1.0 }
                    text {
                        content: format!(
                            "{} {}",
                            archive_count,
                            tr(locale, "个日志文件", "log files")
                        ),
                        font_size: 11.0,
                        font_color: subtle(),
                    }
                }
                text {
                    content: if recording_enabled {
                        tr(
                            locale,
                            "仅记录开启后的内容；停止后不会继续写入。",
                            "Only events after start are recorded; stopping ends capture.",
                        )
                    } else {
                        tr(
                            locale,
                            "点击右上角开始记录，本次应用会话结束后不会自动续开。",
                            "Tap the top-right button to start; recording will not resume automatically next launch.",
                        )
                    },
                    width: "100%",
                    margin_top: 6.0,
                    font_size: 11.0,
                    line_height: 16.0,
                    font_color: subtle(),
                }
            }
            row { height: 16.0 }
            {section_label(tr(locale, "当前日志", "Current logs"))}
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
                        content: if recording_enabled {
                            tr(locale, "暂无诊断日志", "No diagnostic entries")
                        } else {
                            tr(
                                locale,
                                "日志记录未开启，不会保留当前诊断内容。",
                                "Recording is off, so current diagnostics are not retained.",
                            )
                        },
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
                                    content: display_log_timestamp(&entry.timestamp),
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
            row { height: 16.0 }
            {section_label(tr(locale, "历史记录", "History"))}
            if archives.is_empty() {
                column {
                    width: "100%",
                    padding: 24.0,
                    align_items: "start",
                    background_color: surface(),
                    border_width: 1.0,
                    border_color: line(),
                    border_radius: 12.0,
                    text {
                        content: tr(
                            locale,
                            "开启日志记录后会按天生成文件。",
                            "Daily files appear after recording is enabled.",
                        ),
                        width: "100%",
                        font_size: 14.0,
                        font_color: subtle(),
                    }
                }
            } else {
                {archives.into_iter().map(|archive| {
                    let file_name = archive.file_name.clone();
                    let file_for_export = file_name.clone();
                    let file_for_delete = file_name.clone();
                    let file_for_confirm = file_name.clone();
                    let exporting = export_pending.as_deref() == Some(file_name.as_str());
                    let deleting = delete_pending.as_deref() == Some(file_name.as_str());
                    let busy = export_pending.is_some() || delete_pending.is_some();
                    let confirming = selected_delete.as_deref() == Some(file_name.as_str());
                    rsx! {
                        column {
                            key: "{file_name}",
                            width: "100%",
                            margin_bottom: 8.0,
                            padding: 12.0,
                            align_items: "stretch",
                            background_color: surface(),
                            border_width: 1.0,
                            border_color: line(),
                            border_radius: 10.0,
                            text {
                                content: file_name,
                                width: "100%",
                                font_size: 13.0,
                                font_weight: 650,
                                font_color: text_color(),
                                max_lines: 1_i32,
                                text_overflow: "ellipsis",
                            }
                            text {
                                content: if archive.active {
                                    format!(
                                        "{} · {} · {}",
                                        archive.date,
                                        format_bytes(archive.bytes),
                                        tr(locale, "正在写入", "Recording")
                                    )
                                } else {
                                    format!("{} · {}", archive.date, format_bytes(archive.bytes))
                                },
                                width: "100%",
                                margin_top: 4.0,
                                font_size: 11.0,
                                font_color: if archive.active { success() } else { subtle() },
                            }
                            if confirming {
                                text {
                                    content: tr(
                                        locale,
                                        "删除后无法恢复，确定删除此日志？",
                                        "This cannot be undone. Delete this log?",
                                    ),
                                    width: "100%",
                                    margin_top: 10.0,
                                    font_size: 12.0,
                                    font_color: danger(),
                                }
                                row {
                                    width: "100%",
                                    margin_top: 8.0,
                                    justify_content: "end",
                                    FlatButton {
                                        variant: FlatButtonVariant::Ghost,
                                        size: ButtonSize::Sm,
                                        onclick: move |_| pending_delete.set(None),
                                        text {
                                            content: tr(locale, "取消", "Cancel"),
                                            font_size: 12.0,
                                            font_color: text_color(),
                                        }
                                    }
                                    row { width: 8.0 }
                                    FlatButton {
                                        variant: FlatButtonVariant::Destructive,
                                        size: ButtonSize::Sm,
                                        disabled: Some(busy || archive.active),
                                        onclick: move |_| {
                                            pending_delete.set(None);
                                            dispatch(
                                                state,
                                                Action::DeleteLogArchive(file_for_delete.clone()),
                                            );
                                        },
                                        if deleting {
                                            Spinner { size: 14.0, color: Some(destructive_text()) }
                                        } else {
                                            text {
                                                content: tr(locale, "删除", "Delete"),
                                                font_size: 12.0,
                                                font_color: destructive_text(),
                                            }
                                        }
                                    }
                                }
                            } else {
                                row {
                                    width: "100%",
                                    margin_top: 8.0,
                                    justify_content: "end",
                                    FlatButton {
                                        variant: FlatButtonVariant::Outline,
                                        size: ButtonSize::Sm,
                                        disabled: Some(busy),
                                        onclick: move |_| dispatch(
                                            state,
                                            Action::ExportLogArchive(file_for_export.clone()),
                                        ),
                                        if exporting {
                                            Spinner { size: 14.0, color: Some(text_color()) }
                                        } else {
                                            {arkit::icon("download", 14.0, text_color())}
                                            text {
                                                content: tr(locale, "导出", "Export"),
                                                margin_left: 6.0,
                                                font_size: 12.0,
                                                font_color: text_color(),
                                            }
                                        }
                                    }
                                    row { width: 8.0 }
                                    FlatButton {
                                        variant: FlatButtonVariant::Ghost,
                                        size: ButtonSize::Sm,
                                        disabled: Some(busy || archive.active),
                                        onclick: move |_| pending_delete.set(Some(file_for_confirm.clone())),
                                        {arkit::icon("trash-2", 14.0, if archive.active { subtle() } else { danger() })}
                                        text {
                                            content: if archive.active {
                                                tr(locale, "先停止记录", "Stop first")
                                            } else {
                                                tr(locale, "删除", "Delete")
                                            },
                                            margin_left: 6.0,
                                            font_size: 12.0,
                                            font_color: if archive.active { subtle() } else { danger() },
                                        }
                                    }
                                }
                            }
                        }
                    }
                })}
            }
        }
    };

    scaffold(state, Route::Diagnostics {}, actions, body)
}

fn display_log_timestamp(value: &str) -> String {
    let Ok(seconds) = value.parse::<u64>() else {
        return value.to_owned();
    };
    let seconds = seconds % (24 * 60 * 60);
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        (seconds / 60) % 60,
        seconds % 60
    )
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
