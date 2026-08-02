use crate::action::Action;
use crate::i18n::LocaleCatalog;
use crate::settings::{Settings, SettingsChange};
use crate::ui::widgets::button::{ButtonKind, button};
use crate::ui::widgets::dialog::{self, DialogConfig};
use crate::ui::widgets::dropdown::{self, DropdownState};
use eframe::egui;
use rust_i18n::t;

const DIALOG: DialogConfig = DialogConfig {
    id: "settings_window",
    width: 440.0,
    max_height: 300.0,
};

#[derive(Default)]
pub(super) struct SettingsWindow {
    dialog: dialog::DialogState,
    languages: DropdownState,
}

impl SettingsWindow {
    pub(super) fn open(&mut self) {
        if !self.dialog.is_open() {
            self.languages = DropdownState::default();
        }

        self.dialog.open();
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        settings: &Settings,
        locales: &LocaleCatalog,
    ) -> Option<Action> {
        if !self.dialog.is_open() {
            return None;
        }

        let mut change = None;
        let mut action = None;

        dialog::show(
            ui,
            t!("settings.title"),
            &DIALOG,
            &mut self.dialog,
            |ui| {
                Self::show_languages(
                    ui,
                    settings,
                    locales,
                    &mut self.languages,
                    &mut change,
                );

                ui.separator();
                Self::show_themes(ui, settings, &mut change);
                ui.separator();
                Self::show_configuration(ui, &mut action);
            },
        );

        action.or_else(|| change.map(Action::UpdateSettings))
    }

    fn show_configuration(
        ui: &mut egui::Ui,
        action: &mut Option<Action>,
    ) {
        ui.label(t!("settings.configuration"));

        ui.horizontal_wrapped(|ui| {
            for (label, kind, command) in [
                (
                    t!("settings.import"),
                    ButtonKind::Secondary,
                    Action::ImportSettings,
                ),
                (
                    t!("settings.export"),
                    ButtonKind::Secondary,
                    Action::ExportSettings,
                ),
                (
                    t!("settings.reset"),
                    ButtonKind::Danger,
                    Action::ResetSettings,
                ),
            ] {
                if button(ui, label, kind).clicked() {
                    *action = Some(command);
                }
            }
        });
    }

    fn show_languages(
        ui: &mut egui::Ui,
        settings: &Settings,
        locales: &LocaleCatalog,
        state: &mut DropdownState,
        change: &mut Option<SettingsChange>,
    ) {
        ui.label(t!("settings.language"));

        let entries = locales.entries();

        if let Some(index) = dropdown::show(
            ui,
            "language_selector",
            state,
            entries,
            |entry| settings.locale == entry.locale,
            |entry| entry.name.as_str(),
            |entry| entry.locale.as_str(),
        ) {
            *change =
                Some(SettingsChange::Locale(entries[index].locale.clone()));
        }
    }

    fn show_themes(
        ui: &mut egui::Ui,
        settings: &Settings,
        change: &mut Option<SettingsChange>,
    ) {
        ui.label(t!("settings.theme"));

        ui.horizontal(|ui| {
            for (preference, label) in [
                (egui::ThemePreference::System, t!("settings.theme_system")),
                (egui::ThemePreference::Light, t!("settings.theme_light")),
                (egui::ThemePreference::Dark, t!("settings.theme_dark")),
            ] {
                let selected = settings.theme == preference;
                let response = ui.selectable_label(selected, label);

                if !selected && response.clicked() {
                    *change = Some(SettingsChange::ThemePreference(preference));
                }
            }
        });
    }
}
