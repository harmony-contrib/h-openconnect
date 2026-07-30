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
                        tr(
                            current.locale,
                            "查看 OpenConnect 运行日志",
                            "Inspect OpenConnect runtime logs",
                        ),
                    ),
                    settings_route_row(
                        Route::About {},
                        tr(
                            current.locale,
                            "开源信息、组件版本与隐私说明",
                            "Open source, component versions and privacy",
                        ),
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
