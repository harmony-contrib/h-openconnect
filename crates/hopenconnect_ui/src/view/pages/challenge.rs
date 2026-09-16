use super::super::*;
use crate::model::{AuthChallenge, AuthFieldKind};

/// Full-screen modal sheet shown while OpenConnect waits for form values.
///
/// Field labels and input kinds are rendered from the server form without
/// reclassifying names such as `password`, `answer`, or `secondary_password`.
pub(crate) fn auth_challenge_overlay(state: Signal<State>, challenge: AuthChallenge) -> Element {
    let metrics = arkit::use_window_metrics();
    let viewport_height = metrics.content_rect.height as f32 / metrics.scale.max(1.0);
    let bottom_inset = metrics.ime_area.bottom.max(0.0);
    // The portal panel sizes to its content, so a percentage height would
    // depend on that same content. Resolve against the window instead.
    let body_height = if viewport_height > 0.0 {
        (viewport_height - metrics.safe_area.top - metrics.safe_area.bottom - bottom_inset - 72.0)
            .max(120.0)
            * 0.82
    } else {
        360.0
    };
    let locale = state.read().locale;
    let title = sanitize_display_text(
        &challenge
            .message
            .clone()
            .filter(|m| !m.trim().is_empty())
            .or_else(|| challenge.banner.clone().filter(|m| !m.trim().is_empty()))
            .unwrap_or_else(|| translate_ui(locale, tr::challenge_required())),
    );
    let subtitle = translate_ui(locale, tr::challenge_round(challenge.round.to_string()));
    let error = challenge
        .error
        .clone()
        .filter(|e| !e.trim().is_empty())
        .map(|error| sanitize_display_text(&error));
    let values = state.read().challenge_values.clone();
    // Show every non-hidden field, including Unknown (OTP / 动态口令).
    let fields: Vec<_> = challenge
        .fields
        .into_iter()
        .filter(|field| !matches!(field.kind, AuthFieldKind::Hidden))
        .collect();

    rsx! {
        AuthSheet {
            title: translate_ui(locale, tr::challenge_required()),
            bottom_inset,
            on_close: move |_| dispatch(state, Action::CancelChallenge),
            column {
                width: "100%",
                height: body_height,
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
                            font_size: typography::LG,
                            font_weight: 700,
                            font_color: text_color(),
                            max_lines: 3_i32,
                            text_overflow: "ellipsis",
                        }
                        text {
                            content: subtitle,
                            margin_top: 4.0,
                            font_size: typography::XS,
                            font_color: subtle(),
                        }
                    }
                }
                if let Some(error) = error {
                    text {
                        content: error,
                        margin_top: 12.0,
                        font_size: typography::SM,
                        font_color: danger(),
                    }
                }
                row { height: 14.0 }
                column {
                    width: "100%",
                    layout_weight: 1.0,
                    scroll {
                        width: "100%",
                        height: "100%",
                        alignment: "top_start",
                        FieldGroup {
                            {fields.into_iter().map(|field| {
                            let field_key = field.key.clone();
                            let key_for_input = field.key.clone();
                            let render_key = format!(
                                "{}:{}:{}",
                                field.key.form_id.as_deref().unwrap_or(""),
                                field.key.option_index,
                                field.key.option_digest,
                            );
                            let label = if field.label.trim().is_empty() {
                                field.name.clone()
                            } else {
                                field.label.clone()
                            };
                            let current = values.get(&field_key).cloned().unwrap_or_default();
                            let is_select = matches!(field.kind, AuthFieldKind::Select);
                            let input_mode = if matches!(field.kind, AuthFieldKind::Password) {
                                InputMode::Password
                            } else {
                                InputMode::Text
                            };
                            let is_auth_group = field.auth_group;
                            let choices = field.choices.clone();
                            rsx! {
                                Field {
                                    key: "{render_key}",
                                    FieldLabel {
                                        content: if field.required {
                                            format!("{label} *")
                                        } else {
                                            label
                                        },
                                    }
                                    if is_select && !choices.is_empty() {
                                        {
                                            let options: Vec<String> = choices
                                                .iter()
                                                .map(|c| c.label.clone())
                                                .filter(|label| !label.trim().is_empty())
                                                .collect();
                                            let selected_label = choices
                                                .iter()
                                                .find(|c| {
                                                    c.label == current
                                                        || (!is_auth_group && c.name == current)
                                                })
                                                .map(|c| c.label.clone())
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
                                                            .find(|c| c.label == label)
                                                            .map(|c| {
                                                                if is_auth_group {
                                                                    c.label.clone()
                                                                } else {
                                                                    c.name.clone()
                                                                }
                                                            })
                                                            .unwrap_or(label);
                                                        dispatch(
                                                            state,
                                                            Action::SetChallengeField {
                                                                key: key_for_input.clone(),
                                                                value,
                                                            },
                                                        );
                                                    })),
                                                }
                                            }
                                        }
                                    } else {
                                        Input {
                                            value: Some(current),
                                            width: Some("100%".to_owned()),
                                            mode: input_mode,
                                            placeholder: Some(translate_ui(locale, tr::challenge_placeholder())),
                                            on_change: move |value| {
                                                dispatch(
                                                    state,
                                                    Action::SetChallengeField {
                                                        key: key_for_input.clone(),
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
                                content: translate_ui(locale, tr::cancel()),
                                font_size: typography::SM,
                                font_weight: 650,
                                font_color: text_color(),
                            }
                        }
                    }
                    row { width: 10.0 }
                    column {
                        layout_weight: 1.0,
                        FlatButton {
                            variant: FlatButtonVariant::Primary,
                            width: Some("100%".to_owned()),
                            onclick: move |_| dispatch(state, Action::SubmitChallenge),
                            text {
                                content: translate_ui(locale, tr::challenge_submit()),
                                font_size: typography::SM,
                                font_weight: 700,
                                font_color: primary_text(),
                            }
                        }
                    }
                }
            }
        }
    }
}
