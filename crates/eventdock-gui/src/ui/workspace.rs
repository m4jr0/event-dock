use crate::action::TabAction;
use crate::tab::{TabId, TabManager};
use crate::ui::tab_bar::TabBar;
use eframe::egui;
use rust_i18n::t;

const EMPTY_STATE_HEIGHT: f32 = 64.0;

pub(crate) struct Workspace {
    tabs: TabManager,
    tab_bar: TabBar,
}

#[derive(Debug)]
pub(crate) struct TabIdExhausted;

impl Workspace {
    pub(crate) fn new() -> Self {
        Self {
            tabs: TabManager::new(),
            tab_bar: TabBar::default(),
        }
    }

    pub(crate) fn active_tab_id(&self) -> Option<TabId> {
        self.tabs.active_tab_id()
    }

    pub(crate) fn handle_action(
        &mut self,
        action: TabAction,
    ) -> Result<(), TabIdExhausted> {
        match action {
            TabAction::Open => {
                self.tabs
                    .add_tab()
                    .ok_or(TabIdExhausted)?;
            }

            TabAction::Close(id) => {
                self.tabs.remove_tab(id);
            }

            TabAction::Select(id) => {
                self.tabs.select_tab(id);
            }
        }
        Ok(())
    }

    pub(crate) fn show_tab_bar(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Option<TabAction> {
        let action = egui::Panel::top("tab_bar")
            .show(ui, |ui| self.tab_bar.show(ui, &self.tabs))
            .inner;

        if matches!(&action, Some(TabAction::Close(_))) {
            self.tab_bar.on_pointer_close();
        }

        action
    }

    pub(crate) fn show_content(
        &self,
        ui: &mut egui::Ui,
        open_shortcut: &str,
    ) {
        show_empty_state(ui, self.tabs.tabs().is_empty(), open_shortcut);
    }
}

fn show_empty_state(
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
