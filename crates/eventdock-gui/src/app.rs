use crate::action::Action;
use crate::config::AppConfig;
use crate::settings::{Settings, SettingsManager};
use crate::tab::TabManager;
use crate::ui::{shell, tab_bar, theme, windows::Windows};
use eframe::egui;

struct App {
    name: String,
    settings: SettingsManager,
    windows: Windows,
    tabs: TabManager,
}

impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        defaults: Settings,
        name: String,
    ) -> Self {
        theme::install(&cc.egui_ctx);

        let (settings, error) =
            SettingsManager::new(&cc.egui_ctx, defaults, cc.storage);

        let mut windows = Windows::default();

        if let Some(error) = error {
            windows.open_settings();
            windows.open_error(&rust_i18n::t!("settings.load_error"), error);
        }

        Self {
            name,
            settings,
            windows,
            tabs: TabManager::new(),
        }
    }

    fn handle_action(
        &mut self,
        action: Action,
        ctx: &egui::Context,
    ) {
        match action {
            Action::OpenTab => {
                if self.tabs.add_tab().is_none() {
                    self.windows.open_error(
                        "Tab error",
                        "Tab identifier space exhausted".into(),
                    );
                }
            }

            Action::CloseTab(id) => {
                self.tabs.remove_tab(id);
            }

            Action::SelectTab(id) => {
                self.tabs.select_tab(id);
            }

            Action::OpenSettings => self.windows.open_settings(),
            Action::OpenAbout => self.windows.open_about(),
            Action::UpdateSettings(change) => self.settings.update(change, ctx),

            Action::ImportSettings => {
                if let Err(error) = self.settings.import(ctx) {
                    self.windows.open_error(
                        &rust_i18n::t!("settings.import_error"),
                        error,
                    );
                }
            }

            Action::ExportSettings => {
                if let Err(error) = self.settings.export() {
                    self.windows.open_error(
                        &rust_i18n::t!("settings.export_error"),
                        error,
                    );
                }
            }

            Action::ResetSettings => self.settings.reset(ctx),
            Action::Exit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }
}

impl eframe::App for App {
    fn save(
        &mut self,
        storage: &mut dyn eframe::Storage,
    ) {
        if let Err(error) = self.settings.save(storage) {
            self.windows
                .open_error(&rust_i18n::t!("settings.save_error"), error);
        }
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        _frame: &mut eframe::Frame,
    ) {
        let menu_action = egui::Panel::top("menu_bar")
            .show(ui, shell::show)
            .inner;

        if let Some(action) = menu_action {
            self.handle_action(action, ui.ctx());
        }

        let tab_action = egui::Panel::top("tab_bar")
            .show(ui, |ui| tab_bar::show(ui, &self.tabs))
            .inner;

        if let Some(action) = tab_action {
            self.handle_action(action, ui.ctx());
        }

        egui::CentralPanel::default().show(ui, |_ui| {});

        if let Some(action) = self.windows.show(
            ui,
            self.settings.current(),
            self.settings.locales(),
            &self.name,
        ) {
            self.handle_action(action, ui.ctx());
        }
    }
}

pub(crate) fn run(config: AppConfig) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("eventdock")
            .with_inner_size(config.size)
            .with_title(&config.name),

        persist_window: true,

        ..Default::default()
    };

    let name = config.name.clone();

    eframe::run_native(
        &config.name,
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, config.settings, name)))),
    )
}
