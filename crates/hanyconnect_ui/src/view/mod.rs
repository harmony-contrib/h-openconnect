mod pages;
mod route;

use crate::l10n::{strings, tr, UiLocale};
use crate::model::{
    format_bytes, format_duration, AuthMethod, ConnectionLifecycle, ProtocolKind, SoftwareToken,
    SplitTunnelMode, VpnConnection,
};
use crate::platform_callbacks;
use crate::state::{reduce, Action, Command, LanguagePreference, State, ThemePreference};
use arkit::dioxus_core::EventHandler;
use arkit::prelude::*;
use arkit::router::{use_back_handler, use_navigator, use_route, AnimatedOutlet, Router};
use arkit::shadcn::components::{
    Badge, BadgeVariant, BottomNavigation, BottomNavigationItem, ButtonSize, CardContent,
    CardHeader, CardTitle, Field, FieldContent, FieldDescription, FieldOrientation, FieldTitle,
    Form, FormItem, Input, InputMode, RadioGroup, Select, Separator, Sonner, SonnerPosition,
    SonnerToast, Spinner, Switch, Textarea, ToastVariant,
};
use arkit::shadcn::theme::{use_theme, Theme, ThemeMode, ThemePreset, ThemeProvider};
use pages::{
    about_page, appearance_page, auth_challenge_overlay, connection_editor_page, connections_page,
    diagnostics_page, home_page, more_page, statistics_page,
};
use route::Route;
use std::rc::Rc;

fn bg() -> u32 {
    use_theme().colors.background
}

fn surface() -> u32 {
    use_theme().colors.card
}

fn muted() -> u32 {
    use_theme().colors.muted
}

fn text_color() -> u32 {
    use_theme().colors.foreground
}

fn subtle() -> u32 {
    use_theme().colors.muted_foreground
}

fn line() -> u32 {
    use_theme().colors.border
}

fn destructive_text() -> u32 {
    use_theme().colors.destructive_foreground
}

fn success() -> u32 {
    match use_theme().mode {
        ThemeMode::Light => 0xFF16A34A,
        ThemeMode::Dark => 0xFF4ADE80,
    }
}

fn warning() -> u32 {
    match use_theme().mode {
        ThemeMode::Light => 0xFFD97706,
        ThemeMode::Dark => 0xFFFBBF24,
    }
}

fn danger() -> u32 {
    use_theme().colors.destructive
}

fn accent() -> u32 {
    match use_theme().mode {
        ThemeMode::Light => 0xFF1D4ED8,
        ThemeMode::Dark => 0xFF60A5FA,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum FlatButtonVariant {
    #[default]
    Outline,
    Destructive,
    Ghost,
    Accent,
}

#[derive(Props, Clone, PartialEq)]
struct FlatButtonProps {
    #[props(default)]
    variant: FlatButtonVariant,
    #[props(default)]
    size: ButtonSize,
    disabled: Option<bool>,
    /// CSS width (`"100%"`, `"48"`, …). When unset, size defaults apply.
    width: Option<String>,
    onclick: Option<EventHandler<()>>,
    children: Element,
}

#[component]
fn FlatButton(props: FlatButtonProps) -> Element {
    let disabled = props.disabled.unwrap_or(false);
    let onclick = props.onclick;
    let (height, size_width, horizontal_padding) = match props.size {
        ButtonSize::Default => (48.0, None, 20.0),
        ButtonSize::Sm => (36.0, None, 12.0),
        ButtonSize::Lg => (56.0, None, 32.0),
        ButtonSize::Icon => (40.0, Some(40.0), 0.0),
    };
    let (background, foreground, border_width, border_color) = match props.variant {
        FlatButtonVariant::Outline => (surface(), text_color(), 1.0, line()),
        FlatButtonVariant::Destructive => (danger(), destructive_text(), 0.0, danger()),
        FlatButtonVariant::Ghost => (0x00000000, text_color(), 0.0, 0x00000000),
        FlatButtonVariant::Accent => (accent(), 0xFFFFFFFFu32, 0.0, accent()),
    };
    // Explicit CSS width wins over the Icon size default.
    let css_width = props.width.clone();

    rsx! {
        button {
            height: height,
            width: if let Some(w) = css_width {
                w
            } else if let Some(w) = size_width {
                format!("{w}")
            },
            padding_left: horizontal_padding,
            padding_right: horizontal_padding,
            foreground_color: foreground,
            background_color: background,
            border_width: border_width,
            border_color: border_color,
            border_radius: 10.0,
            clip: true,
            opacity: if disabled { 0.5 } else { 1.0 },
            enabled: !disabled,
            onclick: move |_| {
                if !disabled {
                    if let Some(handler) = onclick {
                        handler.call(());
                    }
                }
            },
            row {
                align_items: "center",
                justify_content: "center",
                {props.children}
            }
        }
    }
}

#[component]
pub(crate) fn App() -> Element {
    let state = use_signal(State::new);
    let _state = use_context_provider(move || state);
    let theme = if state.read().theme_dark() {
        Theme::dark(ThemePreset::Zinc)
    } else {
        Theme::light(ThemePreset::Zinc)
    };
    let mut applied_color_mode = use_signal(|| None::<i32>);

    use_effect(move || {
        dispatch(state, Action::Bootstrap);
    });

    use_effect(move || {
        let color_mode = state.read().theme_preference.platform_color_mode();
        if *applied_color_mode.peek() != Some(color_mode) {
            let _ = platform_callbacks::set_color_mode(color_mode);
            applied_color_mode.set(Some(color_mode));
        }
    });

    rsx! {
        ThemeProvider {
            theme,
            Router::<Route> {}
        }
    }
}

#[component]
fn AppShell() -> Element {
    let state = use_context::<Signal<State>>();
    let current = state.read().clone();
    let route = use_route::<Route>();
    let navigator = use_navigator();
    let _back_handler = use_back_handler();
    let nav_items = Route::bottom_routes()
        .iter()
        .map(|route| BottomNavigationItem::new(route.title(current.locale), route.icon()))
        .collect::<Vec<_>>();

    // Short top toasts for key status only (connected / failed / save / validation).
    const TOAST_DURATION_MS: u64 = 2_000;
    let toasts = current
        .toasts
        .iter()
        .map(|item| {
            SonnerToast::new(item.id, item.message.clone())
                .variant(ToastVariant::Info)
                .duration_ms(TOAST_DURATION_MS)
        })
        .collect::<Vec<_>>();

    rsx! {
        stack {
            width: "100%",
            height: "100%",
            background_color: bg(),
            alignment: "top_start",
            column {
                width: "100%",
                height: "100%",
                column {
                    layout_weight: 1.0,
                    width: "100%",
                    AnimatedOutlet::<Route> {}
                }
                if route.parent().is_none() {
                    BottomNavigation {
                        items: nav_items,
                        selected: Some(route.bottom_index()),
                        on_select: move |index| {
                            if let Some(route) = Route::bottom_routes().get(index).cloned() {
                                navigator.replace(route);
                            }
                        }
                    }
                }
            }
            if let Some(challenge) = current.snapshot.pending_auth.clone() {
                {auth_challenge_overlay(state, challenge)}
            }
            Sonner {
                toasts,
                position: SonnerPosition::TopCenter,
                visible_toasts: 2,
                rich_colors: true,
                on_dismiss: move |id| dispatch(state, Action::DismissToast(id)),
            }
        }
    }
}

fn dispatch(mut state: Signal<State>, action: Action) {
    let command = {
        let mut current = state.write();
        reduce(&mut current, action)
    };
    run_command(state, command);
}

fn run_command(state: Signal<State>, command: Command<Action>) {
    let runtime = arkit::tokio_handle();
    for future in command.into_futures() {
        let task = runtime.spawn(future);
        arkit::dioxus_core::spawn_forever(async move {
            if let Ok(action) = task.await {
                dispatch(state, action);
            }
        });
    }
}

fn scaffold(state: Signal<State>, page: Route, actions: Element, body: Element) -> Element {
    let current = state.read().clone();
    let parent = page.parent();
    use_parent_back_handler(parent.clone());
    let navigator = use_navigator();
    // Secondary pages need extra bottom space; bottom-tab pages already sit
    // above BottomNavigation, but keep a small end pad for the last card.
    let end_pad = if parent.is_some() { 28.0 } else { 20.0 };
    let page_title = page.title(current.locale);
    rsx! {
        column {
            layout_weight: 1.0,
            width: "100%",
            background_color: bg(),
            row {
                height: 56.0,
                width: "100%",
                padding_left: 12.0,
                padding_right: 12.0,
                align_items: "center",
                background_color: surface(),
                row {
                    layout_weight: 1.0,
                    align_items: "center",
                    clip: true,
                    if let Some(parent) = parent {
                        FlatButton {
                            variant: FlatButtonVariant::Ghost,
                            size: ButtonSize::Icon,
                            onclick: move |_| {
                                if navigator.can_go_back() {
                                    navigator.go_back();
                                } else {
                                    navigator.push(parent.clone());
                                }
                            },
                            {arkit::icon("arrow-left", 18.0, text_color())}
                        }
                        row { width: 4.0 }
                    }
                    text {
                        content: page_title,
                        font_size: 20.0,
                        line_height: 26.0,
                        font_weight: 700,
                        font_color: text_color(),
                        text_letter_spacing: -0.3,
                        max_lines: 1_i32,
                        text_overflow: "ellipsis",
                    }
                }
                {actions}
            }
            Separator {}
            column {
                layout_weight: 1.0,
                width: "100%",
                scroll {
                    width: "100%",
                    height: "100%",
                    alignment: "top_start",
                    background_color: bg(),
                    scroll_bar: "auto",
                    column {
                        width: "100%",
                        padding_top: 16.0,
                        padding_right: 16.0,
                        padding_bottom: end_pad,
                        padding_left: 16.0,
                        align_items: "stretch",
                        justify_content: "start",
                        {body}
                    }
                }
            }
        }
    }
}

fn use_parent_back_handler(parent: Option<Route>) {
    let navigator = use_navigator();
    let scoped_handler = arkit::dioxus_hooks::use_callback(move |()| {
        let Some(parent) = parent.clone() else {
            return false;
        };
        if navigator.can_go_back() {
            navigator.go_back();
        } else {
            navigator.push(parent);
        }
        true
    });
    let handler: Rc<dyn Fn() -> bool> = Rc::new(move || scoped_handler.call(()));
    let registered_handler = handler.clone();
    let _registration =
        use_hook(|| Rc::new(arkit::register_back_press_handler(registered_handler)));
}

fn card(title: impl Into<String>, subtitle: Option<String>, body: Element) -> Element {
    let title = title.into();
    // Avoid `clip: true` on form cards — it can crop trailing switch rows and
    // multi-line controls on HarmonyOS layout.
    rsx! {
        column {
            width: "100%",
            background_color: surface(),
            border_width: 1.0,
            border_color: line(),
            border_radius: 12.0,
            if let Some(subtitle) = subtitle {
                CardHeader {
                    title: title,
                    description: subtitle,
                }
            } else {
                row {
                    width: "100%",
                    padding_top: 18.0,
                    padding_right: 16.0,
                    padding_bottom: 10.0,
                    padding_left: 16.0,
                    CardTitle { content: title }
                }
            }
            CardContent {
                {body}
            }
        }
    }
}

fn section_label(title: impl Into<String>) -> Element {
    let title = title.into();
    rsx! {
        text {
            content: title,
            margin_left: 4.0,
            margin_bottom: 8.0,
            margin_top: 4.0,
            font_size: 13.0,
            font_weight: 650,
            font_color: subtle(),
        }
    }
}

fn switch_row(
    title: impl Into<String>,
    description: impl Into<String>,
    checked: bool,
    on_change: EventHandler<bool>,
) -> Element {
    let title = title.into();
    let description = description.into();
    rsx! {
        Field {
            orientation: FieldOrientation::Horizontal,
            FieldContent {
                FieldTitle { content: title }
                FieldDescription { content: description, inset: true }
            }
            Switch {
                checked: Some(checked),
                on_change: move |value| on_change.call(value),
            }
        }
    }
}

fn settings_section(title: impl Into<String>, rows: Vec<Element>) -> Element {
    let title = title.into();
    let count = rows.len();
    let rows = rows.into_iter().enumerate().map(|(index, row)| {
        rsx! {
            {row}
            if index + 1 < count { Separator {} }
        }
    });
    rsx! {
        column {
            width: "100%",
            align_items: "start",
            text {
                content: title,
                margin_left: 4.0,
                margin_bottom: 8.0,
                font_size: 13.0,
                font_weight: 650,
                font_color: subtle(),
            }
            column {
                width: "100%",
                padding_left: 14.0,
                padding_right: 8.0,
                background_color: surface(),
                border_width: 1.0,
                border_color: line(),
                border_radius: 12.0,
                clip: true,
                {rows}
            }
        }
    }
}

fn settings_route_row(page: Route, subtitle: impl Into<String>) -> Element {
    let navigator = use_navigator();
    let locale = use_context::<Signal<State>>().read().locale;
    let icon = page.icon();
    let title = page.title(locale);
    let target = page;
    let subtitle = subtitle.into();
    rsx! {
        button {
            width: "100%",
            height: 68.0,
            padding_left: 0.0,
            padding_right: 0.0,
            padding_top: 0.0,
            padding_bottom: 0.0,
            background_color: surface(),
            border_width: 0.0,
            onclick: move |_| {
                navigator.push(target.clone());
            },
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
                        font_size: 15.0,
                        font_weight: 650,
                        font_color: text_color(),
                    }
                    text {
                        content: subtitle,
                        margin_top: 2.0,
                        font_size: 12.0,
                        font_color: subtle(),
                        max_lines: 1,
                        text_overflow: "ellipsis",
                    }
                }
                {arkit::icon("chevron-right", 18.0, subtle())}
            }
        }
    }
}

fn settings_value_row(icon: &str, label: impl Into<String>, value: impl Into<String>) -> Element {
    let label = label.into();
    let value = value.into();
    rsx! {
        row {
            width: "100%",
            height: 58.0,
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
                text { content: label, font_size: 13.0, font_color: subtle() }
                text { content: value, margin_top: 2.0, font_size: 15.0, font_weight: 650, font_color: text_color() }
            }
        }
    }
}

fn lifecycle_label(locale: UiLocale, lifecycle: ConnectionLifecycle) -> &'static str {
    let s = strings(locale);
    match lifecycle {
        ConnectionLifecycle::Disconnected => s.disconnected,
        ConnectionLifecycle::Connecting => s.connecting,
        ConnectionLifecycle::Authenticating => tr(locale, "正在认证…", "Authenticating…"),
        ConnectionLifecycle::Establishing => tr(locale, "正在建立隧道…", "Establishing tunnel…"),
        ConnectionLifecycle::Connected => s.connected,
        ConnectionLifecycle::Disconnecting => s.disconnecting,
        ConnectionLifecycle::Failed => s.failed,
    }
}

fn lifecycle_color(lifecycle: ConnectionLifecycle) -> u32 {
    match lifecycle {
        ConnectionLifecycle::Connected => success(),
        ConnectionLifecycle::Connecting
        | ConnectionLifecycle::Authenticating
        | ConnectionLifecycle::Establishing
        | ConnectionLifecycle::Disconnecting => warning(),
        ConnectionLifecycle::Failed => danger(),
        ConnectionLifecycle::Disconnected => subtle(),
    }
}

fn metric_tile(icon: &str, label: impl Into<String>, value: impl Into<String>) -> Element {
    let label = label.into();
    let value = value.into();
    rsx! {
        column {
            layout_weight: 1.0,
            width: "100%",
            padding: 14.0,
            background_color: surface(),
            border_width: 1.0,
            border_color: line(),
            border_radius: 12.0,
            align_items: "start",
            row {
                width: 34.0,
                height: 34.0,
                align_items: "center",
                justify_content: "center",
                background_color: muted(),
                border_radius: 9.0,
                {arkit::icon(icon, 16.0, accent())}
            }
            text { content: label, margin_top: 12.0, font_size: 12.0, font_color: subtle() }
            text { content: value, margin_top: 4.0, font_size: 17.0, font_weight: 700, font_color: text_color() }
        }
    }
}
