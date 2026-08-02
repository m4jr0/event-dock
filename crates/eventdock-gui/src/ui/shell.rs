use crate::action::Action;
use eframe::egui;
use rust_i18n::t;

pub(crate) fn show(ui: &mut egui::Ui) -> Option<Action> {
    let mut action = None;

    egui::MenuBar::new().ui(ui, |ui| {
        show_file_menu(ui, &mut action);
        show_settings_menu(ui, &mut action);
        show_about_menu(ui, &mut action);
    });

    action
}

fn show_file_menu(
    ui: &mut egui::Ui,
    action: &mut Option<Action>,
) {
    ui.menu_button(t!("menu.file"), |ui| {
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
