use crate::settings::SettingsChange;
use crate::tab::TabId;

pub(crate) enum Action {
    OpenTab,
    CloseTab(TabId),
    SelectTab(TabId),
    OpenSettings,
    OpenAbout,
    UpdateSettings(SettingsChange),
    ImportSettings,
    ExportSettings,
    ResetSettings,
    Exit,
}
