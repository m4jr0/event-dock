mod document;
mod manager;
mod storage;
mod transfer;

pub(crate) use manager::SettingsManager;

use eframe::egui::ThemePreference;

use crate::i18n::Locale;

#[derive(
    Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(default)]
pub(crate) struct Settings {
    pub(crate) locale: Locale,
    #[serde(with = "document::theme_preference")]
    pub(crate) theme: ThemePreference,
}

pub(crate) enum SettingsChange {
    Locale(Locale),
    ThemePreference(ThemePreference),
}
