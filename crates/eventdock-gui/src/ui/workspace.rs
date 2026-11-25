use eframe::egui;
use rust_i18n::t;

const EMPTY_STATE_HEIGHT: f32 = 64.0;

pub(crate) fn show(
    ui: &mut egui::Ui,
    empty: bool,
    open_shortcut: &str,
) {
    if !empty {
        return;
    }

    let top_space = (ui.available_height() - EMPTY_STATE_HEIGHT).max(0.0) * 0.5;
    ui.add_space(top_space);

    ui.vertical_centered(|ui| {
        ui.heading(t!("workspace.empty"));
        ui.label(t!("workspace.open_hint", shortcut = open_shortcut));
    });
}
