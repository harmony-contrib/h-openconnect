use super::*;
use arkit::dioxus_core::VNode;
use arkit::router::dioxus_router;
use arkit::router::Routable;

#[derive(Routable, Clone, PartialEq, Debug)]
pub(super) enum Route {
    #[layout(AppShell)]
    #[route("/")]
    Home {},
    #[route("/connections")]
    Connections {},
    #[route("/connections/edit?:id")]
    ConnectionEditor { id: String },
    #[route("/statistics")]
    Statistics {},
    #[route("/more")]
    More {},
    #[route("/more/appearance")]
    Appearance {},
    #[route("/more/diagnostics")]
    Diagnostics {},
    #[route("/more/about")]
    About {},
}

impl Route {
    pub(super) fn title(&self, locale: UiLocale) -> &'static str {
        let s = strings(locale);
        match self {
            Self::Home {} => s.nav_home,
            Self::Connections {} => s.nav_connections,
            Self::ConnectionEditor { id } => {
                if id.is_empty() {
                    s.add_connection
                } else {
                    s.edit_connection
                }
            }
            Self::Statistics {} => s.nav_statistics,
            Self::More {} => s.nav_more,
            Self::Appearance {} => s.appearance,
            Self::Diagnostics {} => s.diagnostics,
            Self::About {} => s.about,
        }
    }

    pub(super) fn icon(&self) -> &'static str {
        match self {
            Self::Home {} => "shield",
            Self::Connections {} => "server",
            Self::ConnectionEditor { .. } => "pen-line",
            Self::Statistics {} => "activity",
            Self::More {} => "menu",
            Self::Appearance {} => "palette",
            Self::Diagnostics {} => "scroll-text",
            Self::About {} => "badge-info",
        }
    }

    pub(super) fn bottom_index(&self) -> usize {
        match self {
            Self::Home {} => 0,
            Self::Connections {} | Self::ConnectionEditor { .. } => 1,
            Self::Statistics {} => 2,
            Self::More {} | Self::Appearance {} | Self::Diagnostics {} | Self::About {} => 3,
        }
    }

    pub(super) fn parent(&self) -> Option<Self> {
        match self {
            Self::ConnectionEditor { .. } => Some(Self::Connections {}),
            Self::Appearance {} | Self::Diagnostics {} | Self::About {} => Some(Self::More {}),
            _ => None,
        }
    }

    pub(super) fn bottom_routes() -> [Self; 4] {
        [
            Self::Home {},
            Self::Connections {},
            Self::Statistics {},
            Self::More {},
        ]
    }
}

fn state() -> Signal<State> {
    use_context::<Signal<State>>()
}

#[component]
fn Home() -> Element {
    home_page(state())
}

#[component]
fn Connections() -> Element {
    connections_page(state())
}

#[component]
fn ConnectionEditor(id: String) -> Element {
    connection_editor_page(state(), id)
}

#[component]
fn Statistics() -> Element {
    statistics_page(state())
}

#[component]
fn More() -> Element {
    more_page(state())
}

#[component]
fn Appearance() -> Element {
    appearance_page(state())
}

#[component]
fn Diagnostics() -> Element {
    diagnostics_page(state())
}

#[component]
fn About() -> Element {
    about_page(state())
}
