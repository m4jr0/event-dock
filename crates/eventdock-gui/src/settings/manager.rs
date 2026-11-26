use super::{Settings, SettingsChange, storage, transfer};
use crate::action::SettingsAction;
use crate::i18n::{Locale, LocaleCatalog};
use eframe::egui;

pub(crate) struct SettingsManager {
    settings: Settings,
    locales: LocaleCatalog,
    writable: bool,
}

#[derive(Debug)]
pub(crate) enum SettingsActionError {
    Import(String),
    Export(String),
}

impl SettingsManager {
    pub(crate) fn handle_action(
        &mut self,
        action: SettingsAction,
        ctx: &egui::Context,
    ) -> Result<(), SettingsActionError> {
        match action {
            SettingsAction::Update(change) => self.update(change, ctx),

            SettingsAction::Import => self
                .import(ctx)
                .map_err(SettingsActionError::Import)?,

            SettingsAction::Export => self
                .export()
                .map_err(SettingsActionError::Export)?,

            SettingsAction::Reset => self.reset(ctx),
        }

        Ok(())
    }

    pub(crate) fn new(
        ctx: &egui::Context,
        defaults: Settings,
        source: Option<&dyn eframe::Storage>,
    ) -> (Self, Option<String>) {
        let (settings, error) = match storage::load(source) {
            Ok(settings) => (settings.unwrap_or(defaults), None),
            Err(error) => (defaults, Some(error)),
        };

        let mut manager = Self {
            settings,
            locales: LocaleCatalog::new(),
            writable: error.is_none(),
        };

        manager.apply(ctx);

        (manager, error)
    }

    pub(crate) fn current(&self) -> &Settings {
        &self.settings
    }

    pub(crate) fn locales(&self) -> &LocaleCatalog {
        &self.locales
    }

    fn update(
        &mut self,
        change: SettingsChange,
        ctx: &egui::Context,
    ) {
        match change {
            SettingsChange::Locale(locale) => self.settings.locale = locale,

            SettingsChange::ThemePreference(theme) => {
                self.settings.theme = theme
            }
        }

        self.apply(ctx);
    }

    fn apply(
        &mut self,
        ctx: &egui::Context,
    ) {
        if !self
            .locales
            .contains(&self.settings.locale)
        {
            self.settings.locale = Locale::default();
        }

        self.settings.locale.apply();
        ctx.set_theme(self.settings.theme);
    }

    fn replace(
        &mut self,
        settings: Settings,
        ctx: &egui::Context,
    ) {
        self.settings = settings;
        self.apply(ctx);
        self.writable = true;
    }

    fn reset(
        &mut self,
        ctx: &egui::Context,
    ) {
        self.replace(Settings::default(), ctx);
    }

    fn import(
        &mut self,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        if let Some(settings) = transfer::pick_import()? {
            self.replace(settings, ctx);
        }

        Ok(())
    }

    fn export(&self) -> Result<(), String> {
        transfer::pick_export(&self.settings)
    }

    pub(crate) fn save(
        &self,
        target: &mut dyn eframe::Storage,
    ) -> Result<(), String> {
        if self.writable {
            storage::save(target, &self.settings)?;
        }

        Ok(())
    }
}
