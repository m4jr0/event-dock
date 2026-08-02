use crate::settings::SettingsChange;

pub(crate) enum Action {
    OpenSettings,
    OpenAbout,
    UpdateSettings(SettingsChange),
    ImportSettings,
    ExportSettings,
    ResetSettings,
    Exit,
}
