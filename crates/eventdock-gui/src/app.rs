use crate::action::Action;
use crate::config::AppConfig;
use crate::input::{Command, ShortcutManager};
use crate::settings::{Settings, SettingsManager};
use crate::tab::TabManager;
use crate::ui::{shell, tab_bar::TabBar, theme, windows::Windows, workspace};
use eframe::egui;

struct App {
    name: String,
    settings: SettingsManager,
    windows: Windows,
    tabs: TabManager,
    tab_bar: TabBar,
    shortcuts: ShortcutManager,
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
            tab_bar: TabBar::default(),
            shortcuts: ShortcutManager::default(),
        }
    }

    fn handle_command(
        &mut self,
        command: Command,
        ctx: &egui::Context,
    ) {
        let action = match command {
            Command::OpenTab => Some(Action::OpenTab),
            Command::CloseActiveTab => self
                .tabs
                .active_tab_id()
                .map(Action::CloseTab),
        };

        if let Some(action) = action {
            self.handle_action(action, ctx);
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
                        &rust_i18n::t!("tab.error"),
                        rust_i18n::t!("tab.id_exhausted").to_string(),
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
        if let Some(command) = self.shortcuts.consume(ui.ctx()) {
            self.handle_command(command, ui.ctx());
        }

        let menu_action = egui::Panel::top("menu_bar")
            .show(ui, |ui| {
                shell::show(ui, &self.shortcuts, self.tabs.active_tab_id())
            })
            .inner;

        if let Some(action) = menu_action {
            self.handle_action(action, ui.ctx());
        }

        let tab_action = egui::Panel::top("tab_bar")
            .show(ui, |ui| self.tab_bar.show(ui, &self.tabs))
            .inner;

        if let Some(action) = tab_action {
            if let Action::CloseTab(_) = &action {
                self.tab_bar.on_pointer_close();
            }

            self.handle_action(action, ui.ctx());
        }

        let shortcut = self
            .shortcuts
            .label(ui.ctx(), Command::OpenTab);

        egui::CentralPanel::default().show(ui, |ui| {
            workspace::show(ui, self.tabs.tabs().is_empty(), &shortcut);
        });

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
