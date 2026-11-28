use crate::ui::widgets::dialog::{self, DialogConfig};
use eframe::egui;
use rust_i18n::t;
use std::sync::LazyLock;

const LICENSE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../LICENSE"));

const DIALOG: DialogConfig = DialogConfig {
    id: "about_window",
    width: 520.0,
    max_height: 460.0,
};

static REFLOWED_LICENSE: LazyLock<String> =
    LazyLock::new(|| reflow_license(LICENSE));

fn reflow_license(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n");
    normalized
        .split("\n\n")
        .map(|paragraph| {
            paragraph
                .lines()
                .map(str::trim)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPOSITORY: &str = "https://github.com/m4jr0/event-dock";

#[derive(Default)]
pub(super) struct AboutWindow {
    dialog: dialog::DialogState,
}

impl AboutWindow {
    pub(super) fn open(&mut self) {
        self.dialog.open();
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        app_name: &str,
    ) {
        dialog::show(
            ui,
            t!("about.title", name = app_name),
            &DIALOG,
            &mut self.dialog,
            |ui| {
                Self::show_information(ui, app_name);
                ui.separator();
                Self::show_versions(ui);
                ui.separator();
                Self::show_license(ui);
            },
        );
    }

    fn show_information(
        ui: &mut egui::Ui,
        app_name: &str,
    ) {
        ui.vertical_centered(|ui| {
            ui.heading(app_name);
            let width = ui.available_width().min(360.0);

            ui.allocate_ui_with_layout(
                egui::vec2(width, 0.0),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.set_width(width);
                    ui.label(t!("about.description"));
                },
            );

            ui.hyperlink_to(t!("about.repository"), REPOSITORY);
        });
    }

    fn show_versions(ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading(t!("about.components"));
            Self::show_version_table(ui);
        });
    }

    fn show_version_table(ui: &mut egui::Ui) {
        let components = [
            ("eventdock-gui", VERSION),
            ("eventdock", eventdock::VERSION),
            ("eventdock-engine", eventdock_engine::VERSION),
        ];

        let font_size = crate::ui::theme::current(ui)
            .typography
            .body
            + 2.0;

        let name_text = |name: &str| {
            egui::RichText::new(name)
                .strong()
                .size(font_size)
        };

        let version_text = |version: &str| {
            egui::RichText::new(version)
                .code()
                .size(font_size)
        };

        let name_width = components
            .iter()
            .map(|(name, _)| {
                egui::WidgetText::from(name_text(name))
                    .into_galley(
                        ui,
                        Some(egui::TextWrapMode::Extend),
                        f32::INFINITY,
                        egui::TextStyle::Body,
                    )
                    .size()
                    .x
            })
            .fold(0.0_f32, f32::max);

        let version_width = components
            .iter()
            .map(|(_, version)| {
                egui::WidgetText::from(version_text(version))
                    .into_galley(
                        ui,
                        Some(egui::TextWrapMode::Extend),
                        f32::INFINITY,
                        egui::TextStyle::Body,
                    )
                    .size()
                    .x
            })
            .fold(0.0_f32, f32::max);

        let gap = 3.0 * ui.spacing().item_spacing.x;
        let width = name_width + gap + version_width;
        let offset = ((ui.available_width() - width) * 0.5).max(0.0);

        ui.horizontal(|ui| {
            ui.add_space(offset);

            egui::Grid::new("component_versions")
                .num_columns(2)
                .spacing(egui::vec2(gap, ui.spacing().item_spacing.y))
                .min_col_width(0.0)
                .show(ui, |ui| {
                    for (name, version) in components {
                        ui.label(name_text(name));
                        ui.label(version_text(version));
                        ui.end_row();
                    }
                });
        });
    }

    fn show_license(ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading(t!("about.license"));
        });

        ui.label(REFLOWED_LICENSE.as_str());
    }
}
