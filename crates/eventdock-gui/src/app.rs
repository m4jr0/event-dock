use crate::action::{Action, SettingsAction, TabAction};
use crate::config::AppConfig;
use crate::input::{Command, ShortcutManager};
use crate::settings::{Settings, SettingsActionError, SettingsManager};
use crate::ui::{shell, theme, windows::WindowManager, workspace::Workspace};
use eframe::egui;

type UiStage = fn(&mut App, &mut egui::Ui) -> Option<Action>;

const UI_STAGES: &[UiStage] = &[
    App::process_shortcuts,
    App::show_menu,
    App::show_tab_bar,
    App::show_workspace,
    App::show_windows,
];

struct App {
    name: String,
    settings: SettingsManager,
    windows: WindowManager,
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

        let mut windows = WindowManager::default();

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

    fn process_shortcuts(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Option<Action> {
        match self.shortcuts.consume(ui.ctx())? {
            Command::OpenTab => Some(Action::Tab(TabAction::Open)),

            Command::CloseActiveTab => self
                .workspace
                .active_tab_id()
                .map(|id| Action::Tab(TabAction::Close(id))),
        }
    }

    fn show_menu(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Option<Action> {
        egui::Panel::top("menu_bar")
            .show(ui, |ui| {
                shell::show(ui, &self.shortcuts, self.workspace.active_tab_id())
            })
            .inner
    }

    fn show_tab_bar(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Option<Action> {
        self.workspace
            .show_tab_bar(ui)
            .map(Action::Tab)
    }

    fn show_workspace(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Option<Action> {
        let shortcut = self
            .shortcuts
            .label(ui.ctx(), Command::OpenTab);

        egui::CentralPanel::default().show(ui, |ui| {
            self.workspace
                .show_content(ui, &shortcut);
        });

        None
    }

    fn show_windows(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Option<Action> {
        self.windows
            .show(
                ui,
                self.settings.current(),
                self.settings.locales(),
                &self.name,
            )
            .map(Action::Settings)
    }

    fn handle_action(
        &mut self,
        action: Action,
        ctx: &egui::Context,
    ) {
        match action {
            Action::Tab(action) => self.handle_tab_action(action),

            Action::Settings(action) => {
                self.handle_settings_action(action, ctx)
            }

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
        if let Err(error) = self.settings.handle_action(action, ctx) {
            let (summary, details) = match error {
                SettingsActionError::Import(details) => {
                    ("settings.import_error", details)
                }

                SettingsActionError::Export(details) => {
                    ("settings.export_error", details)
                }
            };

            self.windows
                .open_error(&rust_i18n::t!(summary), details);
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
        for stage in UI_STAGES {
            if let Some(action) = stage(self, ui) {
                self.handle_action(action, ui.ctx());
            }
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
