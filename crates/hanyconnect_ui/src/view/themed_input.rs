use super::*;
use arkit::shadcn::theme::{spacing, typography, with_alpha};

/// Application-wide single-line input.
///
/// Arkit's current `Input` styles its surface, placeholder, and caret from the
/// active theme but leaves the native TextInput foreground unset. ArkUI then
/// falls back to the system component color, which can match the themed
/// background. Keep the standard Arkit input behavior while explicitly binding
/// the text foreground to the same theme.
#[derive(Props, Clone, PartialEq)]
pub(super) struct InputProps {
    pub placeholder: Option<String>,
    pub value: Option<String>,
    #[props(default)]
    pub mode: InputMode,
    #[props(default)]
    pub height: Option<f32>,
    pub width: Option<String>,
    #[props(default)]
    pub invalid: bool,
    #[props(default)]
    pub disabled: bool,
    #[props(default)]
    pub read_only: bool,
    pub on_change: Option<EventHandler<String>>,
    pub on_click: Option<EventHandler<()>>,
}

#[component]
pub(super) fn Input(props: InputProps) -> Element {
    let theme = use_theme();
    let input_type = match props.mode {
        InputMode::Text => "text",
        InputMode::Password => "password",
        InputMode::Number => "number",
    };
    let input_filter = if props.mode == InputMode::Number {
        Some("[0-9]")
    } else {
        None
    };
    let on_change = props.on_change;
    let on_click = props.on_click;

    rsx! {
        textinput {
            value: if let Some(value) = props.value { value },
            placeholder: if let Some(placeholder) = props.placeholder { placeholder },
            input_type,
            input_filter: if let Some(filter) = input_filter { filter },
            show_password_icon: props.mode == InputMode::Password,
            placeholder_color: with_alpha(theme.colors.muted_foreground, 0x80),
            caret_color: if props.read_only {
                0x00000000
            } else {
                theme.colors.primary
            },
            font_color: theme.colors.foreground,
            font_size: typography::LG,
            line_height: 22.5,
            height: props.height.unwrap_or(48.0),
            border_style: "solid",
            border_width: 1.0,
            border_color: if props.invalid {
                theme.colors.destructive
            } else {
                theme.colors.input
            },
            border_radius: theme.radii.md,
            background_color: theme.colors.background,
            opacity: if props.disabled { 0.5 } else { 1.0 },
            enabled: !props.disabled,
            focusable: !props.read_only,
            focus_on_touch: !props.read_only,
            padding_top: spacing::XXS,
            padding_right: spacing::MD,
            padding_bottom: spacing::XXS,
            padding_left: spacing::MD,
            width: if let Some(width) = props.width { width },
            on_change: move |event| {
                if !props.disabled && !props.read_only {
                    if let Some(handler) = on_change {
                        let value = event.data().string_value.clone();
                        handler.call(if props.mode == InputMode::Number {
                            value
                                .chars()
                                .filter(char::is_ascii_digit)
                                .collect::<String>()
                        } else {
                            value
                        });
                    }
                }
            },
            onclick: move |_| {
                if !props.disabled {
                    if let Some(handler) = on_click {
                        handler.call(());
                    }
                }
            },
        }
    }
}
