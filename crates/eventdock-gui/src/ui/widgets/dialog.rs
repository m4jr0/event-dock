use super::button::{ButtonKind, button};
use eframe::egui;
use rust_i18n::t;

pub(crate) struct DialogConfig {
    pub(crate) id: &'static str,
    pub(crate) width: f32,
    pub(crate) max_height: f32,
}

#[derive(Default)]
pub(crate) struct DialogState {
    is_open: bool,
    just_opened: bool,
}

impl DialogState {
    pub(crate) fn open(&mut self) {
        self.just_opened |= !self.is_open;
        self.is_open = true;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.is_open
    }
}

pub(crate) fn show(
    ui: &mut egui::Ui,
    title: impl Into<egui::WidgetText>,
    config: &DialogConfig,
    state: &mut DialogState,
    content: impl FnOnce(&mut egui::Ui),
) {
    if !state.is_open() {
        return;
    }

    let metrics = &crate::ui::theme::current(ui).metrics;
    let viewport = ui.ctx().content_rect();
    let inset = 2.0 * metrics.spacing_large;

    let width = config
        .width
        .min((viewport.width() - inset).max(1.0));

    let max_height = config
        .max_height
        .min((viewport.height() - inset).max(1.0));

    // Reserve space for the title bar, window margins, separator and footer.
    // The scroll area then shrinks to its contents, up to this maximum.
    let body_height = (max_height
        - 2.0 * metrics.control_height
        - 4.0 * metrics.spacing_medium)
        .max(0.0);

    let mut is_open = true;
    let mut close_requested = false;

    let window = egui::Window::new(title)
        .id(egui::Id::unique(config.id))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .movable(false)
        .collapsible(false)
        .resizable(false)
        .min_width(width)
        .max_width(width)
        .max_height(max_height)
        .open(&mut is_open);

    window.show(ui, |ui| {
        let mut scroll_area = egui::ScrollArea::vertical()
            .max_height(body_height)
            .auto_shrink([false, true]);

        if state.just_opened {
            scroll_area = scroll_area.vertical_scroll_offset(0.0);
        }

        scroll_area.show(ui, content);
        ui.separator();
        close_requested = show_footer(ui);
    });

    state.is_open = is_open && !close_requested;
    state.just_opened = false;
}

pub(crate) fn show_footer(ui: &mut egui::Ui) -> bool {
    ui.allocate_ui_with_layout(
        egui::vec2(
            ui.available_width(),
            crate::ui::theme::current(ui)
                .metrics
                .control_height,
        ),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| button(ui, t!("dialog.close"), ButtonKind::Secondary).clicked(),
    )
    .inner
}
