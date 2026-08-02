mod about;
mod error;
mod settings;

use crate::action::Action;
use crate::i18n::LocaleCatalog;
use crate::settings::Settings;
use eframe::egui;

#[derive(Default)]
pub(crate) struct Windows {
    settings: settings::SettingsWindow,
    about: about::AboutWindow,
    error: error::ErrorWindow,
}

impl Windows {
    pub(crate) fn open_error(
        &mut self,
        summary: &str,
        details: String,
    ) {
        self.error
            .open(summary.to_owned(), details);
    }

    pub(crate) fn open_settings(&mut self) {
        self.settings.open();
    }

    pub(crate) fn open_about(&mut self) {
        self.about.open();
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        settings: &Settings,
        locales: &LocaleCatalog,
        app_name: &str,
    ) -> Option<Action> {
        let action = self
            .settings
            .show(ui, settings, locales);
        self.about.show(ui, app_name);
        self.error.show(ui);
        action
    }
}
