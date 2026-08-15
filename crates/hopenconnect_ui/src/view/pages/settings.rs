use super::super::*;

pub(crate) fn more_page(state: Signal<State>) -> Element {
    let current = state.read().clone();
    let body = rsx! {
        column {
            width: "100%",
            {settings_section(
                translate_ui(current.locale, tr::settings_preferences()),
                vec![
                    settings_route_row(
                        Route::Appearance {},
                        translate_ui(current.locale, tr::settings_language_theme()),
                    ),
                ],
            )}
            row { height: 16.0 }
            {settings_section(
                translate_ui(current.locale, tr::settings_operations()),
                vec![
                    settings_route_row(
                        Route::Diagnostics {},
                        translate_ui(current.locale, tr::settings_logs()),
                    ),
                    settings_route_row(
                        Route::About {},
                        translate_ui(current.locale, tr::settings_about()),
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

    let system_language = translate_ui(locale, tr::system());
    let simplified_chinese = "简体中文".to_owned();
    let english = "English".to_owned();
    let selected_language = match current.language_preference {
        LanguagePreference::System => system_language.clone(),
        LanguagePreference::ZhCn => simplified_chinese.clone(),
        LanguagePreference::En => english.clone(),
    };
    let language_system_option = system_language.clone();
    let language_chinese_option = simplified_chinese.clone();

    let system_theme = translate_ui(locale, tr::system());
    let light_theme = translate_ui(locale, tr::light());
    let dark_theme = translate_ui(locale, tr::dark());
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
                translate_ui(locale, tr::language()),
                Some(translate_ui(locale, tr::settings_language_hint())),
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
                translate_ui(locale, tr::theme()),
                Some(translate_ui(locale, tr::settings_theme_hint())),
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
