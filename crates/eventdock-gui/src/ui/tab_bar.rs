use crate::action::Action;
use crate::tab::TabManager;
use eframe::egui;

const TAB_WIDTH: f32 = 112.0;
const CLOSE_WIDTH: f32 = 22.0;

/// Render the workspace without mutating its model.
pub(crate) fn show(
    ui: &mut egui::Ui,
    tabs: &TabManager,
) -> Option<Action> {
    let mut action = None;

    ui.horizontal(|ui| {
        // Keep the new-tab button accessible even when the strip overflows.
        if ui
            .button("+")
            .on_hover_text("New tab")
            .clicked()
        {
            action = Some(Action::OpenTab);
        }

        egui::ScrollArea::horizontal()
            .id_salt("workspace_tabs")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for tab in tabs.tabs() {
                        let id = tab.id();
                        ui.push_id(id, |ui| {
                            let selected = tabs.active_tab_id() == Some(id);
                            let height = ui.spacing().interact_size.y;
                            let response = ui.add_sized(
                                [TAB_WIDTH, height],
                                egui::Button::new("New tab").selected(selected),
                            );

                            if response.middle_clicked() {
                                action = Some(Action::CloseTab(id));
                            } else if response.clicked() {
                                action = Some(Action::SelectTab(id));
                            }

                            let close = ui
                                .add_sized(
                                    [CLOSE_WIDTH, height],
                                    egui::Button::new("×"),
                                )
                                .on_hover_text("Close tab");
                            if close.clicked() || close.middle_clicked() {
                                action = Some(Action::CloseTab(id));
                            }
                        });
                    }
                });
            });
    });

    action
}
