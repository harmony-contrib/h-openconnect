//! Locale state type shared by the app runtime (pure Rust, no arkit link).

/// Locale state used by the app runtime (profile preference, platform sync).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum UiLocale {
    #[default]
    ZhCn,
    En,
}

impl UiLocale {
    pub(crate) fn from_tag(tag: &str) -> Self {
        let lower = tag.to_ascii_lowercase();
        if lower.starts_with("zh") {
            Self::ZhCn
        } else {
            Self::En
        }
    }

    /// The catalog locale id (matches `tr::CATALOG` in `i18n.rs`).
    pub(crate) fn language_tag(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::En => "en-US",
        }
    }
}
