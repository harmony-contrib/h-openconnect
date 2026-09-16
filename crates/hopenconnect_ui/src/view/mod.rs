mod pages;
mod route;

use crate::bridge;
use crate::i18n::{tr, translate_ui};
use crate::locale::UiLocale;
use crate::log_filter::{matches_log_filter_normalized, normalize_log_query, LogLevelFilter};
use crate::model::{
    format_bytes, format_duration, sanitize_display_text, AuthMethod, ConnectionLifecycle,
    ProtocolKind, SoftwareToken, SplitTunnelMode, VpnConnection,
};
use crate::state::{reduce, Action, Command, LanguagePreference, State, ThemePreference};
use crate::time_format;
use arkit::dioxus_core::EventHandler;
use arkit::prelude::*;
use arkit::router::{use_back_handler, use_navigator, use_route, AnimatedOutlet, Router};
use arkit::shadcn::components::{
    Badge, BadgeVariant, BottomNavigation, BottomNavigationItem, BottomSheet, Button, ButtonSize,
    ButtonVariant, Card, CardContent, CardHeader, CardTitle, Dialog, DialogFooter, DialogHeader,
    Field, FieldContent, FieldDescription, FieldGroup, FieldLabel, FieldOrientation, FieldTitle,
    Input, InputMode, RadioGroup, Select, Separator, Sonner, SonnerPosition, SonnerToast, Spinner,
    Switch, TabsList, TabsTrigger, Textarea, ToastVariant,
};
use arkit::shadcn::theme::{
    control, radius, spacing, typography, use_theme, Theme, ThemeMode, ThemePreset, ThemeProvider,
};
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
    use_theme().colors.primary
}

fn primary_text() -> u32 {
    use_theme().colors.primary_foreground
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum FlatButtonVariant {
    #[default]
    Outline,
    Primary,
    Destructive,
    Ghost,
}

impl FlatButtonVariant {
    fn to_button_variant(self) -> ButtonVariant {
        match self {
            Self::Outline => ButtonVariant::Outline,
            Self::Primary => ButtonVariant::Default,
            Self::Destructive => ButtonVariant::Destructive,
            Self::Ghost => ButtonVariant::Ghost,
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct FlatButtonProps {
    #[props(default)]
    variant: FlatButtonVariant,
    #[props(default)]
    size: ButtonSize,
    disabled: Option<bool>,
    width: Option<String>,
    onclick: Option<EventHandler<()>>,
    children: Element,
}

/// App action names mapped to upstream shadcn variants; styling stays upstream.
#[component]
fn FlatButton(props: FlatButtonProps) -> Element {
    rsx! {
        Button {
            variant: props.variant.to_button_variant(),
            size: props.size,
            disabled: props.disabled,
            width: props.width,
            onclick: props.onclick,
            {props.children}
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct FlatSegmentedProps {
    options: Vec<String>,
    selected: String,
    on_change: EventHandler<String>,
}

/// Controlled app selection using upstream shadcn tab styling.
#[component]
fn FlatSegmented(props: FlatSegmentedProps) -> Element {
    let runtime = arkit::use_runtime_handle();
    rsx! {
        TabsList {
            for option in props.options {
                TabsTrigger {
                    key: "{option}",
                    label: option.clone(),
                    active: option == props.selected,
                    on_press: {
                        let runtime = runtime.clone();
                        let on_change = props.on_change;
                        move |_| {
                            let next = option.clone();
                            runtime.queue_ui(move || on_change.call(next));
                        }
                    },
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct FlatDialogProps {
    open: bool,
    on_close: EventHandler<()>,
    children: Element,
}

/// Use the upstream dialog for focus, dismissal, motion and panel tokens.
#[component]
fn FlatDialog(props: FlatDialogProps) -> Element {
    use_modal_back_handler(props.open, props.on_close);
    rsx! {
        Dialog {
            open: Some(props.open),
            on_close: Some(props.on_close),
            {props.children}
        }
    }
}

/// Keep native Back inside the topmost modal instead of navigating its page.
fn use_modal_back_handler(open: bool, on_close: EventHandler<()>) {
    let callback = arkit::dioxus_hooks::use_callback(move |()| {
        if open {
            on_close.call(());
        }
        open
    });
    let runtime = arkit::use_runtime_handle();
    let _registration = use_hook(move || {
        Rc::new(runtime.register_back_handler(Rc::new(move || callback.call(()))))
    });
}

#[component]
fn AuthSheet(
    title: String,
    bottom_inset: f32,
    on_close: EventHandler<()>,
    children: Element,
) -> Element {
    use_modal_back_handler(true, on_close);
    rsx! {
        BottomSheet {
            title,
            open: Some(true),
            show_header: Some(false),
            on_close,
            {children}
            if bottom_inset > 0.0 {
                // BottomSheet's portal stays anchored to the physical viewport.
                // Reserve the obscured part of that viewport so the actual form
                // and its actions remain above the software keyboard.
                row {
                    width: "100%",
                    height: bottom_inset,
                }
            }
        }
    }
}

#[allow(non_snake_case)]
pub(crate) fn App(initial_safe_area: bridge::InitialSafeArea) -> Element {
    use_context_provider(move || initial_safe_area);
    let state = use_signal(State::new);
    let _state = use_context_provider(move || state);
    let runtime = arkit::use_runtime_handle();
    let theme = if state.read().theme_dark() {
        Theme::dark(ThemePreset::Zinc)
    } else {
        Theme::light(ThemePreset::Zinc)
    };
    let mut applied_color_mode = use_signal(|| None::<i32>);

    let _ = APP_TOKIO_HANDLE.set(runtime.tokio());

    use_effect(move || {
        dispatch(state, Action::Bootstrap);
    });

    use_effect(move || {
        let color_mode = state.read().theme_preference.platform_color_mode();
        if *applied_color_mode.peek() != Some(color_mode) {
            let _ = bridge::set_color_mode(color_mode);
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
    let initial_safe_area = use_context::<bridge::InitialSafeArea>().0;
    let window_metrics = arkit::use_window_metrics();
    let safe_area = if window_metrics.content_rect.is_empty() {
        initial_safe_area
    } else {
        window_metrics.safe_area
    };
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
            padding_top: safe_area.top,
            padding_right: safe_area.right,
            padding_bottom: safe_area.bottom,
            padding_left: safe_area.left,
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

static APP_TOKIO_HANDLE: std::sync::OnceLock<tokio::runtime::Handle> = std::sync::OnceLock::new();

fn app_tokio_handle() -> tokio::runtime::Handle {
    APP_TOKIO_HANDLE
        .get()
        .expect("App must install the runtime tokio handle before dispatch")
        .clone()
}

fn dispatch(mut state: Signal<State>, action: Action) {
    let command = {
        let mut current = state.write();
        reduce(&mut current, action)
    };
    run_command(state, command);
}

fn run_command(state: Signal<State>, command: Command<Action>) {
    let tokio = app_tokio_handle();
    for future in command.into_futures() {
        let task = tokio.spawn(future);
        arkit::dioxus_core::spawn_forever(async move {
            if let Ok(action) = task.await {
                dispatch(state, action);
            }
        });
    }
}

fn scaffold(state: Signal<State>, page: Route, actions: Element, body: Element) -> Element {
    scaffold_layout(state, page, actions, body, true)
}

fn fixed_scaffold(state: Signal<State>, page: Route, actions: Element, body: Element) -> Element {
    scaffold_layout(state, page, actions, body, false)
}

fn scaffold_layout(
    state: Signal<State>,
    page: Route,
    actions: Element,
    body: Element,
    scrollable: bool,
) -> Element {
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
                        font_size: typography::XL,
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
                if scrollable {
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
                } else {
                    column {
                        layout_weight: 1.0,
                        width: "100%",
                        padding_top: 16.0,
                        padding_right: 16.0,
                        padding_bottom: 16.0,
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
        use_hook(|| Rc::new(arkit::use_runtime_handle().register_back_handler(registered_handler)));
}

fn card(title: impl Into<String>, subtitle: Option<String>, body: Element) -> Element {
    let title = title.into();
    // Avoid `clip: true` on form cards — it can crop trailing switch rows and
    // multi-line controls on HarmonyOS layout.
    rsx! {
        Card {
            shadow: Some(false),
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

fn empty_state(
    icon: &'static str,
    title: impl Into<String>,
    subtitle: impl Into<String>,
) -> Element {
    let title = title.into();
    let subtitle = subtitle.into();
    let theme = use_theme();
    rsx! {
        Card {
            shadow: Some(false),
            column {
                width: "100%",
                height: 190.0,
                padding: spacing::XXL,
                align_items: "center",
                justify_content: "center",
                row {
                    width: 48.0,
                    height: 48.0,
                    align_items: "center",
                    justify_content: "center",
                    background_color: theme.colors.muted,
                    border_radius: theme.radii.xl,
                    {arkit::icon(icon, 22.0, theme.colors.muted_foreground)}
                }
                text {
                    content: title,
                    margin_top: spacing::MD,
                    font_size: typography::MD,
                    line_height: 22.0,
                    font_weight: 600,
                    font_color: theme.colors.foreground,
                }
                text {
                    content: subtitle,
                    margin_top: spacing::XXS,
                    font_size: typography::SM,
                    line_height: 20.0,
                    font_color: theme.colors.muted_foreground,
                    text_align: "center",
                }
            }
        }
    }
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let prefix = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
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
                font_size: typography::SM,
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
                border_radius: radius::LG,
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
            button_type: "normal",
            width: "100%",
            height: 68.0,
            border_radius: 0.0,
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
                    border_radius: radius::LG,
                    {arkit::icon(icon, 16.0, text_color())}
                }
                column {
                    layout_weight: 1.0,
                    margin_left: 12.0,
                    align_items: "start",
                    text {
                        content: title,
                        font_size: typography::MD,
                        font_weight: 650,
                        font_color: text_color(),
                    }
                    text {
                        content: subtitle,
                        margin_top: 2.0,
                        font_size: typography::XS,
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
                border_radius: radius::LG,
                {arkit::icon(icon, 16.0, text_color())}
            }
            column {
                layout_weight: 1.0,
                margin_left: 12.0,
                align_items: "start",
                text { content: label, font_size: typography::SM, font_color: subtle() }
                text { content: value, margin_top: 2.0, font_size: typography::MD, font_weight: 650, font_color: text_color() }
            }
        }
    }
}

fn lifecycle_label(locale: UiLocale, lifecycle: ConnectionLifecycle) -> String {
    match lifecycle {
        ConnectionLifecycle::Disconnected => translate_ui(locale, tr::disconnected()),
        ConnectionLifecycle::Connecting => translate_ui(locale, tr::connecting()),
        ConnectionLifecycle::Authenticating => translate_ui(locale, tr::lifecycle_authenticating()),
        ConnectionLifecycle::Establishing => translate_ui(locale, tr::lifecycle_establishing()),
        ConnectionLifecycle::Connected => translate_ui(locale, tr::connected()),
        ConnectionLifecycle::Disconnecting => translate_ui(locale, tr::disconnecting()),
        ConnectionLifecycle::Failed => translate_ui(locale, tr::failed()),
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
            border_radius: radius::LG,
            align_items: "start",
            row {
                width: 34.0,
                height: 34.0,
                align_items: "center",
                justify_content: "center",
                background_color: muted(),
                border_radius: radius::LG,
                {arkit::icon(icon, 16.0, accent())}
            }
            text { content: label, margin_top: 12.0, font_size: typography::XS, font_color: subtle() }
            text { content: value, margin_top: 4.0, font_size: typography::LG, font_weight: 700, font_color: text_color() }
        }
    }
}
