use super::super::*;
use arkit::ohos_arkui_binding::{
    common::node::ArkUINode, types::attribute::ArkUINodeAttributeType,
};
use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

pub(crate) fn diagnostics_page(state: Signal<State>) -> Element {
    let mut log_query = use_signal(String::new);
    let mut log_filter = use_signal(|| LogLevelFilter::All);
    let mut history_open = use_signal(|| false);
    let mut selected_log = use_signal(|| None::<VirtualLogRow>);
    let mut delete_archive = use_signal(|| None::<String>);
    let current = state.read().clone();
    let locale = current.locale;
    let recording_enabled = current.log_recording.enabled;
    let recording_pending = current.log_recording_pending;
    let export_pending = current.log_archive_export_pending.clone();
    let delete_pending = current.log_archive_delete_pending.clone();
    let current_tab = tr(locale, "当前日志", "Current").to_owned();
    let history_tab = tr(locale, "历史记录", "History").to_owned();
    let tab_options = vec![current_tab.clone(), history_tab.clone()];
    let selected_tab = if history_open() {
        history_tab.clone()
    } else {
        current_tab.clone()
    };
    let query_value = log_query();
    let normalized_query = normalize_log_query(&query_value);
    let filter_value = log_filter();
    let all_label = strings(locale).logs_level_all.to_owned();
    let info_label = "Info".to_owned();
    let warn_label = "Warn".to_owned();
    let error_label = "Error".to_owned();
    let debug_label = "Debug".to_owned();
    let filter_options = vec![
        all_label.clone(),
        info_label.clone(),
        warn_label.clone(),
        error_label.clone(),
        debug_label.clone(),
    ];
    debug_assert_eq!(filter_options.len(), LogLevelFilter::ALL.len());
    let selected_filter = match filter_value {
        LogLevelFilter::All => all_label.clone(),
        LogLevelFilter::Info => info_label.clone(),
        LogLevelFilter::Warning => warn_label.clone(),
        LogLevelFilter::Error => error_label.clone(),
        LogLevelFilter::Debug => debug_label.clone(),
    };
    let total_log_count = current.snapshot.diagnostics.len();
    let logs = current
        .snapshot
        .diagnostics
        .iter()
        .filter(|log| matches_log_filter_normalized(log, filter_value, &normalized_query))
        .rev()
        .cloned()
        .map(|log| {
            let color = match log.level.to_ascii_lowercase().as_str() {
                "error" => danger(),
                "warning" | "warn" => warning(),
                "info" => success(),
                _ => subtle(),
            };
            VirtualLogRow {
                meta: format!(
                    "{}  ·  {}",
                    log.level.to_uppercase(),
                    time_format::format_unix_seconds(&log.timestamp).unwrap_or(log.timestamp),
                ),
                preview: truncate_text(&log.message.replace(['\n', '\r'], " "), 150),
                message: log.message,
                color,
            }
        })
        .collect::<Vec<_>>();
    let empty = logs.is_empty();
    let shown_log_count = logs.len();
    let palette = VirtualLogPalette {
        surface: surface(),
        foreground: text_color(),
        muted_foreground: subtle(),
        border: line(),
        danger: danger(),
    };
    let selected_log_value = selected_log();
    let delete_archive_value = delete_archive();
    let archives_empty = current.log_recording.archives.is_empty();
    let archives = current
        .log_recording
        .archives
        .iter()
        .cloned()
        .map(|archive| {
            let updated_at = archive
                .updated_at
                .as_deref()
                .and_then(time_format::format_unix_seconds)
                .unwrap_or_else(|| archive.date.clone());
            let detail = format!("{} · {}", format_bytes(archive.bytes), updated_at);
            VirtualLogArchiveRow {
                exporting: export_pending.as_deref() == Some(archive.file_name.as_str()),
                deleting: delete_pending.as_deref() == Some(archive.file_name.as_str()),
                export_disabled: export_pending.is_some() || delete_pending.is_some(),
                delete_disabled: archive.active
                    || export_pending.is_some()
                    || delete_pending.is_some(),
                detail: if archive.active {
                    format!(
                        "{detail} · {}",
                        tr(
                            locale,
                            "正在写入，停止记录后可删除",
                            "Recording; stop before deleting"
                        )
                    )
                } else {
                    detail
                },
                file_name: archive.file_name,
            }
        })
        .collect::<Vec<_>>();
    let body = rsx! {
        column {
            width: "100%",
            height: "100%",
            row {
                width: "100%",
                height: 32.0,
                align_items: "center",
                text {
                    content: if recording_enabled {
                        tr(locale, "正在记录并按天保存", "Recording and saving daily")
                    } else {
                        tr(locale, "日志记录已关闭", "Log recording is off")
                    },
                    font_size: 12.0,
                    font_weight: 600,
                    font_color: if recording_enabled { success() } else { subtle() },
                }
                row { layout_weight: 1.0 }
                text {
                    content: format!(
                        "{} {}",
                        current.log_recording.archives.len(),
                        tr(locale, "个日志文件", "log files")
                    ),
                    font_size: 11.0,
                    font_color: subtle(),
                }
            }
            row { height: 6.0 }
            row {
                width: "100%",
                justify_content: "center",
                FlatSegmented {
                    options: tab_options,
                    selected: selected_tab,
                    on_change: move |value: String| {
                        history_open.set(value == history_tab);
                    },
                }
            }
            row { height: 12.0 }
            if history_open() {
                row {
                    layout_weight: 1.0,
                    width: "100%",
                    if archives_empty {
                        {empty_state(
                            "history",
                            tr(locale, "暂无历史日志", "No log history"),
                            tr(
                                locale,
                                "开启日志记录后会按天生成文件",
                                "Daily files appear after recording is enabled"
                            ),
                        )}
                    } else {
                        VirtualLogArchiveList {
                            items: archives,
                            palette,
                            on_export: move |file_name: String| {
                                dispatch(state, Action::ExportLogArchive(file_name));
                            },
                            on_delete: move |file_name: String| {
                                delete_archive.set(Some(file_name));
                            },
                        }
                    }
                }
            } else {
                Input {
                    value: Some(query_value),
                    placeholder: Some(strings(locale).logs_search_placeholder.to_owned()),
                    width: Some("100%".into()),
                    on_change: move |value| log_query.set(value),
                }
                row { height: 12.0 }
                row {
                    width: "100%",
                    justify_content: "center",
                    FlatSegmented {
                        options: filter_options,
                        selected: selected_filter,
                        on_change: move |value: String| {
                            let filter = if value == info_label {
                                LogLevelFilter::Info
                            } else if value == warn_label {
                                LogLevelFilter::Warning
                            } else if value == error_label {
                                LogLevelFilter::Error
                            } else if value == debug_label {
                                LogLevelFilter::Debug
                            } else {
                                LogLevelFilter::All
                            };
                            log_filter.set(filter);
                        },
                    }
                }
                row {
                    width: "100%",
                    height: 32.0,
                    align_items: "center",
                    text {
                        content: format!(
                            "{} / {} {}",
                            shown_log_count,
                            total_log_count,
                            tr(locale, "条日志", "logs")
                        ),
                        font_size: 11.0,
                        font_color: subtle(),
                    }
                    row { layout_weight: 1.0 }
                    if !empty {
                        text {
                            content: tr(
                                locale,
                                "点击日志查看全文",
                                "Tap a log for details"
                            ),
                            font_size: 11.0,
                            font_color: subtle(),
                        }
                    }
                }
                row {
                    layout_weight: 1.0,
                    width: "100%",
                    if empty {
                        {empty_state(
                            "scroll-text",
                            strings(locale).logs_empty_title,
                            if recording_enabled {
                                strings(locale).logs_empty_subtitle
                            } else {
                                tr(
                                    locale,
                                    "点击右上角开始记录日志",
                                    "Tap the top-right button to start recording"
                                )
                            },
                        )}
                    } else {
                        VirtualLogList {
                            items: logs,
                            palette,
                            on_open: move |row: VirtualLogRow| selected_log.set(Some(row)),
                        }
                    }
                }
            }
        }
    };
    let action = rsx! {
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
    let page = fixed_scaffold(state, Route::Diagnostics {}, action, body);
    rsx! {
        {page}
        if let Some(log) = selected_log_value {
            {log_detail_dialog(locale, log, selected_log)}
        }
        if let Some(file_name) = delete_archive_value {
            {log_archive_delete_dialog(state, locale, file_name, delete_archive)}
        }
    }
}

fn log_archive_delete_dialog(
    state: Signal<State>,
    locale: UiLocale,
    file_name: String,
    mut selected: Signal<Option<String>>,
) -> Element {
    let delete_file_name = file_name.clone();
    rsx! {
        FlatDialog {
            open: true,
            on_close: move |_| selected.set(None),
            DialogHeader {
                title: tr(locale, "删除历史日志？", "Delete log history?").to_owned(),
                description: Some(format!(
                    "{} · {}",
                    file_name,
                    tr(locale, "此操作无法撤销", "This cannot be undone")
                )),
            }
            row { height: 20.0 }
            DialogFooter {
                row {
                    width: "100%",
                    FlatButton {
                        variant: FlatButtonVariant::Outline,
                        onclick: move |_| selected.set(None),
                        text {
                            content: tr(locale, "取消", "Cancel"),
                            font_size: 13.0,
                            font_weight: 600,
                            font_color: text_color(),
                        }
                    }
                    row { layout_weight: 1.0 }
                    FlatButton {
                        variant: FlatButtonVariant::Destructive,
                        onclick: move |_| {
                            selected.set(None);
                            dispatch(state, Action::DeleteLogArchive(delete_file_name.clone()));
                        },
                        text {
                            content: tr(locale, "删除", "Delete"),
                            font_size: 13.0,
                            font_weight: 600,
                            font_color: destructive_text(),
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct VirtualLogRow {
    meta: String,
    message: String,
    preview: String,
    color: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct VirtualLogPalette {
    surface: u32,
    foreground: u32,
    muted_foreground: u32,
    border: u32,
    danger: u32,
}

#[derive(Clone)]
struct VirtualLogRenderState {
    items: Vec<VirtualLogRow>,
    palette: VirtualLogPalette,
    on_open: EventHandler<VirtualLogRow>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct VirtualLogArchiveRow {
    file_name: String,
    detail: String,
    exporting: bool,
    deleting: bool,
    export_disabled: bool,
    delete_disabled: bool,
}

#[derive(Clone)]
struct VirtualLogArchiveRenderState {
    items: Vec<VirtualLogArchiveRow>,
    palette: VirtualLogPalette,
    on_export: EventHandler<String>,
    on_delete: EventHandler<String>,
}

#[component]
fn VirtualLogArchiveList(
    items: Vec<VirtualLogArchiveRow>,
    palette: VirtualLogPalette,
    on_export: EventHandler<String>,
    on_delete: EventHandler<String>,
) -> Element {
    let item_keys = items
        .iter()
        .map(|item| {
            let mut hasher = DefaultHasher::new();
            item.hash(&mut hasher);
            palette.hash(&mut hasher);
            hasher.finish()
        })
        .collect::<Vec<_>>();
    let render_state = use_hook(|| {
        Rc::new(RefCell::new(VirtualLogArchiveRenderState {
            items: items.clone(),
            palette,
            on_export,
            on_delete,
        }))
    });
    *render_state.borrow_mut() = VirtualLogArchiveRenderState {
        items,
        palette,
        on_export,
        on_delete,
    };
    let render_state_for_adapter = render_state.clone();
    let handle = use_virtual_node_adapter_items_keyed(VirtualKind::List, item_keys, move |index| {
        let state = render_state_for_adapter.borrow();
        render_virtual_log_archive_row(
            &state.items[index as usize],
            state.palette,
            render_state_for_adapter.clone(),
        )
    });
    let attach_handle = handle.clone();
    use_layout_frame_node(move |host_node, _frame| {
        let _ = attach_handle.attach(&host_node);
    });

    rsx! {
        list {
            width: "100%",
            height: "100%",
            list_cached_count: 12_i32,
        }
    }
}

#[component]
fn VirtualLogList(
    items: Vec<VirtualLogRow>,
    palette: VirtualLogPalette,
    on_open: EventHandler<VirtualLogRow>,
) -> Element {
    let item_keys = items
        .iter()
        .map(|item| {
            let mut hasher = DefaultHasher::new();
            item.hash(&mut hasher);
            palette.hash(&mut hasher);
            hasher.finish()
        })
        .collect::<Vec<_>>();
    let render_state = use_hook(|| {
        Rc::new(RefCell::new(VirtualLogRenderState {
            items: items.clone(),
            palette,
            on_open,
        }))
    });
    *render_state.borrow_mut() = VirtualLogRenderState {
        items,
        palette,
        on_open,
    };
    let render_state_for_adapter = render_state.clone();
    let handle = use_virtual_node_adapter_items_keyed(VirtualKind::List, item_keys, move |index| {
        let state = render_state_for_adapter.borrow();
        render_virtual_log_row(&state.items[index as usize], state.palette, state.on_open)
    });
    let attach_handle = handle.clone();
    use_layout_frame_node(move |host_node, _frame| {
        let _ = attach_handle.attach(&host_node);
    });

    rsx! {
        list {
            width: "100%",
            height: "100%",
            list_cached_count: 18_i32,
        }
    }
}

fn render_virtual_log_archive_row(
    item: &VirtualLogArchiveRow,
    palette: VirtualLogPalette,
    interaction_state: Rc<RefCell<VirtualLogArchiveRenderState>>,
) -> arkit::ohos_arkui_binding::common::error::ArkUIResult<ArkUINode> {
    let title = virtual_log_text(
        item.file_name.clone(),
        14.0,
        6,
        palette.foreground,
        20.0,
        1,
        0.0,
    )?;
    let detail = virtual_log_text(
        item.detail.clone(),
        11.0,
        4,
        palette.muted_foreground,
        16.0,
        1,
        4.0,
    )?;
    let content = NodeBuilder::new("column")?
        .attr(ArkUINodeAttributeType::LayoutWeight, 1.0_f32)?
        .attr(ArkUINodeAttributeType::ColumnAlignItems, 0_i32)?
        .attr(ArkUINodeAttributeType::ColumnJustifyContent, 2_i32)?
        .child(title)?
        .child(detail)?
        .build();
    let export_color = if item.export_disabled && !item.exporting {
        palette.muted_foreground
    } else {
        palette.foreground
    };
    let export_file_name = item.file_name.clone();
    let export_state = interaction_state.clone();
    let export_action = virtual_log_archive_action(
        if item.exporting { "…" } else { "↓" },
        export_color,
        if item.exporting {
            "exporting log"
        } else {
            "export log"
        },
        move || {
            let state = export_state.borrow();
            if state
                .items
                .iter()
                .find(|item| item.file_name == export_file_name)
                .is_some_and(|item| !item.export_disabled)
            {
                state.on_export.call(export_file_name.clone());
            }
        },
    )?;
    let delete_color = if item.delete_disabled && !item.deleting {
        palette.muted_foreground
    } else {
        palette.danger
    };
    let delete_file_name = item.file_name.clone();
    let delete_state = interaction_state.clone();
    let delete_action = virtual_log_archive_action(
        if item.deleting { "…" } else { "×" },
        delete_color,
        if item.deleting {
            "deleting log"
        } else if item.delete_disabled {
            "stop recording before deleting this log"
        } else {
            "delete log"
        },
        move || {
            let state = delete_state.borrow();
            if state
                .items
                .iter()
                .find(|item| item.file_name == delete_file_name)
                .is_some_and(|item| !item.delete_disabled)
            {
                state.on_delete.call(delete_file_name.clone());
            }
        },
    )?;
    Ok(NodeBuilder::new("row")?
        .percent_width(1.0)?
        .height(72.0)?
        .background_color(format!("#{:08x}", palette.surface))?
        .padding([8.0, 8.0, 8.0, 14.0])?
        .margin([0.0, 0.0, 7.0, 0.0])?
        .attr(ArkUINodeAttributeType::BorderWidth, vec![1.0; 4])?
        .attr(ArkUINodeAttributeType::BorderColor, palette.border)?
        .attr(ArkUINodeAttributeType::BorderRadius, vec![9.0; 4])?
        .attr(ArkUINodeAttributeType::Clip, true)?
        .attr(ArkUINodeAttributeType::RowAlignItems, 1_i32)?
        .attr(
            ArkUINodeAttributeType::AccessibilityText,
            format!("{}，{}", item.file_name, item.detail),
        )?
        .child(content)?
        .child(export_action)?
        .child(delete_action)?
        .build())
}

fn virtual_log_archive_action(
    content: &str,
    color: u32,
    accessibility: &str,
    on_click: impl Fn() + 'static,
) -> arkit::ohos_arkui_binding::common::error::ArkUIResult<ArkUINode> {
    Ok(NodeBuilder::new("text")?
        .width(40.0)?
        .height(40.0)?
        .font_size(if content == "…" { 18.0 } else { 20.0 })?
        .font_color(format!("#{color:08x}"))?
        .text_content(content.to_owned())?
        .attr(ArkUINodeAttributeType::FontWeight, 5_i32)?
        .attr(ArkUINodeAttributeType::TextAlign, 1_i32)?
        .attr(ArkUINodeAttributeType::TextLineHeight, 40.0_f32)?
        .attr(ArkUINodeAttributeType::TextMaxLines, 1_i32)?
        .attr(
            ArkUINodeAttributeType::AccessibilityText,
            accessibility.to_owned(),
        )?
        .on_click(on_click)?
        .build())
}

fn render_virtual_log_row(
    item: &VirtualLogRow,
    palette: VirtualLogPalette,
    on_open: EventHandler<VirtualLogRow>,
) -> arkit::ohos_arkui_binding::common::error::ArkUIResult<ArkUINode> {
    let meta = virtual_log_text(item.meta.clone(), 10.0, 5, item.color, 15.0, 1, 0.0)?;
    let message = virtual_log_text(
        item.preview.clone(),
        12.0,
        4,
        palette.foreground,
        17.0,
        2,
        4.0,
    )?;
    let node = NodeBuilder::new("column")?
        .percent_width(1.0)?
        .height(76.0)?
        .background_color(format!("#{:08x}", palette.surface))?
        .padding([9.0, 11.0, 9.0, 11.0])?
        .margin([0.0, 0.0, 7.0, 0.0])?
        .attr(ArkUINodeAttributeType::BorderWidth, vec![1.0; 4])?
        .attr(ArkUINodeAttributeType::BorderColor, palette.border)?
        .attr(ArkUINodeAttributeType::BorderRadius, vec![9.0; 4])?
        .attr(ArkUINodeAttributeType::Clip, true)?
        .attr(ArkUINodeAttributeType::ColumnAlignItems, 0_i32)?
        .attr(
            ArkUINodeAttributeType::AccessibilityText,
            format!("{}，{}", item.meta, item.message),
        )?
        .child(meta)?
        .child(message)?;
    let item = item.clone();
    Ok(node.on_click(move || on_open.call(item.clone()))?.build())
}

fn virtual_log_text(
    content: String,
    size: f32,
    weight: i32,
    color: u32,
    line_height: f32,
    max_lines: i32,
    padding_top: f32,
) -> arkit::ohos_arkui_binding::common::error::ArkUIResult<ArkUINode> {
    Ok(NodeBuilder::new("text")?
        .percent_width(1.0)?
        .font_size(size)?
        .font_color(format!("#{color:08x}"))?
        .text_content(content)?
        .padding([padding_top, 0.0, 0.0, 0.0])?
        .attr(ArkUINodeAttributeType::FontWeight, weight)?
        .attr(ArkUINodeAttributeType::TextLineHeight, line_height)?
        .attr(ArkUINodeAttributeType::TextMaxLines, max_lines)?
        .attr(ArkUINodeAttributeType::TextOverflow, 2_i32)?
        .build())
}

fn log_detail_dialog(
    locale: UiLocale,
    log: VirtualLogRow,
    mut selected: Signal<Option<VirtualLogRow>>,
) -> Element {
    let detail_height = match log.message.chars().count() {
        0..=160 => 120.0,
        161..=420 => 200.0,
        _ => 300.0,
    };
    rsx! {
        FlatDialog {
            open: true,
            on_close: move |_| selected.set(None),
            DialogHeader {
                title: tr(locale, "日志详情", "Log details").to_owned(),
                description: Some(log.meta),
            }
            row { height: 14.0 }
            scroll {
                width: "100%",
                height: detail_height,
                alignment: "top-start",
                scroll_bar: "off",
                background_color: muted(),
                border_radius: 9.0,
                column {
                    width: "100%",
                    padding: 12.0,
                    align_items: "start",
                    justify_content: "start",
                    text {
                        content: log.message,
                        width: "100%",
                        font_size: 12.0,
                        line_height: 19.0,
                        font_color: text_color(),
                    }
                }
            }
        }
    }
}
