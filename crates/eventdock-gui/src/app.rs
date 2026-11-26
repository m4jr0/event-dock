use crate::action::{Action, SettingsAction, TabAction};
use crate::config::AppConfig;
use crate::input::{Command, ShortcutManager};
use crate::settings::{Settings, SettingsManager};
use crate::ui::{shell, theme, windows::Windows, workspace::Workspace};
use eframe::egui;

struct App {
    name: String,
    settings: SettingsManager,
    windows: Windows,
    workspace: Workspace,
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
            workspace: Workspace::new(),
            shortcuts: ShortcutManager::default(),
        }
    }

    fn handle_command(
        &mut self,
        command: Command,
        ctx: &egui::Context,
    ) {
        let action = match command {
            Command::OpenTab => Some(Action::Tab(TabAction::Open)),
            Command::CloseActiveTab => self
                .workspace
                .active_tab_id()
                .map(|id| Action::Tab(TabAction::Close(id))),
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
            Action::Tab(action) => self.handle_tab_action(action),
            Action::OpenSettings => self.windows.open_settings(),
            Action::OpenAbout => self.windows.open_about(),
            Action::Exit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        }
    }

    fn handle_tab_action(
        &mut self,
        action: TabAction,
    ) {
        if self
            .workspace
            .handle_action(action)
            .is_err()
        {
            self.windows.open_error(
                &rust_i18n::t!("tab.error"),
                rust_i18n::t!("tab.id_exhausted").to_string(),
            );
        }
    }

    fn handle_settings_action(
        &mut self,
        action: SettingsAction,
        ctx: &egui::Context,
    ) {
        match action {
            SettingsAction::Update(change) => self.settings.update(change, ctx),
            SettingsAction::Import => {
                if let Err(error) = self.settings.import(ctx) {
                    self.windows.open_error(
                        &rust_i18n::t!("settings.import_error"),
                        error,
                    );
                }
            }
            SettingsAction::Export => {
                if let Err(error) = self.settings.export() {
                    self.windows.open_error(
                        &rust_i18n::t!("settings.export_error"),
                        error,
                    );
                }
            }
            SettingsAction::Reset => self.settings.reset(ctx),
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
                shell::show(ui, &self.shortcuts, self.workspace.active_tab_id())
            })
            .inner;

        if let Some(action) = menu_action {
            self.handle_action(action, ui.ctx());
        }

        if let Some(action) = self.workspace.show_tab_bar(ui) {
            self.handle_tab_action(action);
        }

        let shortcut = self
            .shortcuts
            .label(ui.ctx(), Command::OpenTab);

        egui::CentralPanel::default().show(ui, |ui| {
            self.workspace
                .show_content(ui, &shortcut);
        });

        if let Some(action) = self.windows.show(
            ui,
            self.settings.current(),
            self.settings.locales(),
            &self.name,
        ) {
            self.handle_settings_action(action, ui.ctx());
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
