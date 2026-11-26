use crate::settings::SettingsChange;
use crate::tab::TabId;

pub(crate) enum Action {
    Tab(TabAction),
    Settings(SettingsAction),
    OpenSettings,
    OpenAbout,
    Exit,
}

pub(crate) enum TabAction {
    Open,
    Close(TabId),
    Select(TabId),
    Move(TabId, usize),
}

pub(crate) enum SettingsAction {
    Update(SettingsChange),
    Import,
    Export,
    Reset,
}
