mod fonts;
mod metrics;
mod palette;
mod presets;
mod style;
mod typography;

use eframe::egui;
use metrics::Metrics;
use palette::Palette;
use typography::Typography;

pub(crate) struct Theme {
    pub(crate) palette: Palette,
    pub(crate) metrics: Metrics,
    pub(crate) typography: Typography,
}

pub(crate) fn current(ui: &egui::Ui) -> &'static Theme {
    if ui.visuals().dark_mode {
        &presets::DARK
    } else {
        &presets::LIGHT
    }
}

pub(crate) fn install(ctx: &egui::Context) {
    fonts::install(ctx);
    ctx.set_style_of(egui::Theme::Dark, style::build(&presets::DARK, true));
    ctx.set_style_of(egui::Theme::Light, style::build(&presets::LIGHT, false));
}
