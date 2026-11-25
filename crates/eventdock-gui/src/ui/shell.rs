use crate::action::Action;
use crate::input::{Command, ShortcutManager};
use crate::tab::TabId;
use eframe::egui;
use rust_i18n::t;

pub(crate) fn show(
    ui: &mut egui::Ui,
    shortcuts: &ShortcutManager,
    active_tab: Option<TabId>,
) -> Option<Action> {
    let mut action = None;

    egui::MenuBar::new().ui(ui, |ui| {
        show_file_menu(ui, shortcuts, active_tab, &mut action);
        show_settings_menu(ui, &mut action);
        show_about_menu(ui, &mut action);
    });

    action
}

fn show_file_menu(
    ui: &mut egui::Ui,
    shortcuts: &ShortcutManager,
    active_tab: Option<TabId>,
    action: &mut Option<Action>,
) {
    ui.menu_button(t!("menu.file"), |ui| {
        shortcut_menu_item(
            ui,
            t!("tab.new"),
            shortcuts,
            Command::OpenTab,
            Some(Action::OpenTab),
            action,
        );

        shortcut_menu_item(
            ui,
            t!("tab.close"),
            shortcuts,
            Command::CloseActiveTab,
            active_tab.map(Action::CloseTab),
            action,
        );

        ui.separator();

        menu_item(ui, t!("menu.exit"), Action::Exit, action);
    });
}

fn show_settings_menu(
    ui: &mut egui::Ui,
    action: &mut Option<Action>,
) {
    ui.menu_button(t!("menu.settings"), |ui| {
        menu_item(ui, t!("menu.preferences"), Action::OpenSettings, action);
    });
}

fn show_about_menu(
    ui: &mut egui::Ui,
    action: &mut Option<Action>,
) {
    ui.menu_button(t!("menu.about"), |ui| {
        menu_item(ui, t!("menu.about_app"), Action::OpenAbout, action);
    });
}

fn menu_item(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    action: Action,
    output: &mut Option<Action>,
) {
    if ui.button(label).clicked() {
        *output = Some(action);
    }
}

fn shortcut_menu_item(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    shortcuts: &ShortcutManager,
    command: Command,
    action: Option<Action>,
    output: &mut Option<Action>,
) {
    let shortcut = shortcuts.label(ui.ctx(), command);
    let button = egui::Button::new(label).shortcut_text(shortcut);

    if ui
        .add_enabled(action.is_some(), button)
        .clicked()
    {
        *output = action;
    }
}
