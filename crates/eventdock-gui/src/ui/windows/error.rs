use crate::ui::{theme, widgets::dialog};
use eframe::egui;
use rust_i18n::t;

const WIDTH: f32 = 360.0;
const DETAILS_MAX_HEIGHT: f32 = 160.0;

struct ErrorMessage {
    summary: String,
    details: String,
}

#[derive(Default)]
pub(super) struct ErrorWindow {
    message: Option<ErrorMessage>,
}

impl ErrorWindow {
    pub(super) fn open(
        &mut self,
        summary: String,
        details: String,
    ) {
        self.message = Some(ErrorMessage { summary, details });
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
    ) {
        let Some(message) = &self.message else {
            return;
        };
        let response =
            egui::Modal::new(egui::Id::unique("error_window")).show(ui, |ui| {
                ui.set_width(WIDTH);

                Self::show_message(ui, message);
                ui.separator();
                dialog::show_footer(ui)
            });

        if response.inner || response.should_close() {
            self.message = None;
        }
    }

    fn show_message(
        ui: &mut egui::Ui,
        message: &ErrorMessage,
    ) {
        let palette = &theme::current(ui).palette;

        ui.heading(
            egui::RichText::new(t!("dialog.error")).color(palette.status.error),
        );

        ui.separator();

        let metrics = &theme::current(ui).metrics;

        ui.add_space(metrics.spacing_medium);
        ui.heading(egui::RichText::new(&message.summary).strong());
        ui.add_space(metrics.spacing_large);
        ui.label(t!("dialog.details"));
        ui.add_space(metrics.spacing_small);

        Self::show_details(ui, &message.details);

        ui.add_space(metrics.spacing_medium);
    }

    fn show_details(
        ui: &mut egui::Ui,
        details: &str,
    ) {
        let theme = theme::current(ui);
        let palette = &theme.palette;
        let metrics = &theme.metrics;

        egui::Frame::new()
            .fill(palette.surface.background)
            .stroke(egui::Stroke::new(
                metrics.border_width,
                palette.surface.border,
            ))
            .corner_radius(egui::CornerRadius::same(metrics.corner_radius))
            .inner_margin(egui::Margin::same(
                metrics.spacing_medium.round() as i8
            ))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                egui::ScrollArea::vertical()
                    .max_height(DETAILS_MAX_HEIGHT)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(details).monospace());
                    });
            });
    }
}
