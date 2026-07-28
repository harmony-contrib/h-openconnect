use super::super::*;
use crate::model::{AuthChallenge, AuthFieldKind};

/// Full-screen modal sheet shown while OpenConnect waits for form values.
///
/// All multi-round challenge fields use **visible plain-text** inputs (including
/// password/token kinds) so OTP / SMS codes are easy to verify while typing.
pub(crate) fn auth_challenge_overlay(state: Signal<State>, challenge: AuthChallenge) -> Element {
    let locale = state.read().locale;
    let s = strings(locale);
    let title = challenge
        .message
        .clone()
        .filter(|m| !m.trim().is_empty())
        .or_else(|| challenge.banner.clone().filter(|m| !m.trim().is_empty()))
        .unwrap_or_else(|| {
            tr(
                locale,
                "服务器需要额外认证",
                "Server requires additional authentication",
            )
            .to_owned()
        });
    let subtitle = tr(
        locale,
        "第 {n} 轮认证表单 · 输入内容明文可见",
        "Authentication form · round {n} · plain text",
    )
    .replace("{n}", &challenge.round.to_string());
    let error = challenge.error.clone().filter(|e| !e.trim().is_empty());
    let values = state.read().challenge_values.clone();
    // Show every non-hidden field, including Unknown (OTP / 动态口令).
    let fields: Vec<_> = challenge
        .fields
        .into_iter()
        .filter(|field| !matches!(field.kind, AuthFieldKind::Hidden))
        .collect();

    rsx! {
        column {
            width: "100%",
            height: "100%",
            background_color: 0x99000000u32,
            align_items: "center",
            justify_content: "end",
            column {
                width: "100%",
                background_color: surface(),
                border_radius: 16.0,
                padding_top: 18.0,
                padding_right: 16.0,
                padding_bottom: 20.0,
                padding_left: 16.0,
                align_items: "stretch",
                row {
                    width: "100%",
                    align_items: "center",
                    {arkit::icon("shield", 20.0, accent())}
                    column {
                        layout_weight: 1.0,
                        margin_left: 10.0,
                        align_items: "start",
                        text {
                            content: title,
                            font_size: 17.0,
                            font_weight: 700,
                            font_color: text_color(),
                            max_lines: 3_i32,
                            text_overflow: "ellipsis",
                        }
                        text {
                            content: subtitle,
                            margin_top: 4.0,
                            font_size: 12.0,
                            font_color: subtle(),
                        }
                    }
                }
                if let Some(error) = error {
                    text {
                        content: error,
                        margin_top: 12.0,
                        font_size: 13.0,
                        font_color: danger(),
                    }
                }
                row { height: 14.0 }
                scroll {
                    width: "100%",
                    height: 280.0,
                    alignment: "top_start",
                    column {
                        width: "100%",
                        align_items: "stretch",
                        {fields.into_iter().map(|field| {
                            let name = field.name.clone();
                            let name_for_input = field.name.clone();
                            let label = if field.label.trim().is_empty() {
                                field.name.clone()
                            } else {
                                field.label.clone()
                            };
                            let current = values.get(&name).cloned().unwrap_or_default();
                            let is_select = matches!(field.kind, AuthFieldKind::Select);
                            let choices = field.choices.clone();
                            rsx! {
                                column {
                                    key: "{name}",
                                    width: "100%",
                                    margin_bottom: 12.0,
                                    text {
                                        content: if field.required {
                                            format!("{label} *")
                                        } else {
                                            label
                                        },
                                        margin_bottom: 6.0,
                                        font_size: 13.0,
                                        font_weight: 600,
                                        font_color: subtle(),
                                    }
                                    if is_select && !choices.is_empty() {
                                        {
                                            let options: Vec<String> = choices
                                                .iter()
                                                .map(|c| {
                                                    if c.label.is_empty() {
                                                        c.name.clone()
                                                    } else {
                                                        c.label.clone()
                                                    }
                                                })
                                                .collect();
                                            let selected_label = choices
                                                .iter()
                                                .find(|c| c.name == current || c.label == current)
                                                .map(|c| {
                                                    if c.label.is_empty() {
                                                        c.name.clone()
                                                    } else {
                                                        c.label.clone()
                                                    }
                                                })
                                                .or_else(|| options.first().cloned())
                                                .unwrap_or_default();
                                            let choices_for_handler = choices.clone();
                                            let default_sel = selected_label.clone();
                                            rsx! {
                                                Select {
                                                    options,
                                                    selected: Some(selected_label),
                                                    default_selected: default_sel,
                                                    open: None,
                                                    default_open: false,
                                                    on_open_change: None,
                                                    on_select: Some(EventHandler::new(move |label: String| {
                                                        let value = choices_for_handler
                                                            .iter()
                                                            .find(|c| c.label == label || c.name == label)
                                                            .map(|c| c.name.clone())
                                                            .unwrap_or(label);
                                                        dispatch(
                                                            state,
                                                            Action::SetChallengeField {
                                                                name: name_for_input.clone(),
                                                                value,
                                                            },
                                                        );
                                                    })),
                                                }
                                            }
                                        }
                                    } else {
                                        // Always plain-text (visible) — never password mask.
                                        Input {
                                            value: Some(current),
                                            width: Some("100%".to_owned()),
                                            placeholder: Some(tr(
                                                locale,
                                                "在此输入（明文可见）",
                                                "Type here (visible)",
                                            ).to_owned()),
                                            on_change: move |value| {
                                                dispatch(
                                                    state,
                                                    Action::SetChallengeField {
                                                        name: name_for_input.clone(),
                                                        value,
                                                    },
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        })}
                    }
                }
                row { height: 12.0 }
                row {
                    width: "100%",
                    column {
                        layout_weight: 1.0,
                        FlatButton {
                            variant: FlatButtonVariant::Outline,
                            width: Some("100%".to_owned()),
                            onclick: move |_| dispatch(state, Action::CancelChallenge),
                            text {
                                content: s.cancel,
                                font_size: 14.0,
                                font_weight: 650,
                                font_color: text_color(),
                            }
                        }
                    }
                    row { width: 10.0 }
                    column {
                        layout_weight: 1.0,
                        FlatButton {
                            variant: FlatButtonVariant::Accent,
                            width: Some("100%".to_owned()),
                            onclick: move |_| dispatch(state, Action::SubmitChallenge),
                            text {
                                content: s.challenge_submit,
                                font_size: 14.0,
                                font_weight: 700,
                                font_color: 0xFFFFFFFFu32,
                            }
                        }
                    }
                }
            }
        }
    }
}
