use crate::action::TabAction;
use crate::tab::{Tab, TabId};
use eframe::egui;
use rust_i18n::t;

const CLOSE_WIDTH: f32 = 22.0;
const TITLE_PADDING: f32 = 4.0;
const CLOSE_PADDING: f32 = 2.0;
const CLOSE_SYMBOL: &str = "×";

pub(crate) struct TabWidget<'a> {
    tab: &'a Tab,
    selected: bool,
}

impl<'a> TabWidget<'a> {
    pub(crate) fn new(
        tab: &'a Tab,
        selected: bool,
    ) -> Self {
        Self { tab, selected }
    }

    pub(crate) fn show(
        self,
        ui: &mut egui::Ui,
        width: f32,
    ) -> Option<TabAction> {
        let layout = TabLayout::allocate(ui, width);
        let interaction = TabInteraction::detect(ui, &layout);
        let style = TabStyle::resolve(ui, self.selected, &interaction);

        paint_background(ui, &layout, &style);
        paint_title(ui, &layout, &style);
        paint_close_button(ui, &layout, &style, &interaction);

        interaction.action(self.tab.id())
    }
}

struct TabLayout {
    bounds: egui::Rect,
    body: egui::Rect,
    close: egui::Rect,
}

impl TabLayout {
    fn allocate(
        ui: &mut egui::Ui,
        width: f32,
    ) -> Self {
        let height = ui.spacing().interact_size.y;

        let (bounds, _) = ui.allocate_exact_size(
            egui::vec2(width, height),
            egui::Sense::hover(),
        );

        let close = egui::Rect::from_min_max(
            egui::pos2(bounds.right() - CLOSE_WIDTH, bounds.top()),
            bounds.right_bottom(),
        );

        let body = egui::Rect::from_min_max(bounds.min, close.left_bottom());

        Self {
            bounds,
            body,
            close,
        }
    }
}

struct TabInteraction {
    body: egui::Response,
    close: egui::Response,
}

impl TabInteraction {
    fn detect(
        ui: &egui::Ui,
        layout: &TabLayout,
    ) -> Self {
        let id = ui.scope_id();

        let body =
            ui.interact(layout.body, id.with("body"), egui::Sense::click());

        let close = ui
            .interact(layout.close, id.with("close"), egui::Sense::click())
            .on_hover_text(t!("tab.close"));

        Self { body, close }
    }

    fn hovered(&self) -> bool {
        self.body.hovered() || self.close.hovered()
    }

    fn close_highlighted(&self) -> bool {
        self.close.hovered() || self.close.has_focus()
    }

    fn action(
        &self,
        id: TabId,
    ) -> Option<TabAction> {
        if self.close.clicked()
            || self.close.middle_clicked()
            || self.body.middle_clicked()
        {
            Some(TabAction::Close(id))
        } else if self.body.clicked() {
            Some(TabAction::Select(id))
        } else {
            None
        }
    }
}

struct TabStyle {
    fill: egui::Color32,
    stroke: egui::Stroke,
    text_color: egui::Color32,
    corner_radius: egui::CornerRadius,
}

impl TabStyle {
    fn resolve(
        ui: &egui::Ui,
        selected: bool,
        interaction: &TabInteraction,
    ) -> Self {
        let visuals = ui.visuals();

        let widget = if selected {
            &visuals.widgets.active
        } else if interaction.hovered() {
            &visuals.widgets.hovered
        } else {
            &visuals.widgets.inactive
        };

        let (fill, stroke, text_color) = if selected {
            (
                visuals.selection.bg_fill,
                visuals.selection.stroke,
                visuals.selection.stroke.color,
            )
        } else {
            (widget.bg_fill, widget.bg_stroke, widget.fg_stroke.color)
        };

        Self {
            fill,
            stroke,
            text_color,
            corner_radius: widget.corner_radius,
        }
    }
}

fn paint_background(
    ui: &egui::Ui,
    layout: &TabLayout,
    style: &TabStyle,
) {
    ui.painter().rect(
        layout.bounds,
        style.corner_radius,
        style.fill,
        style.stroke,
        egui::StrokeKind::Inside,
    );
}

fn paint_title(
    ui: &egui::Ui,
    layout: &TabLayout,
    style: &TabStyle,
) {
    ui.painter()
        .with_clip_rect(
            layout
                .body
                .shrink2(egui::vec2(TITLE_PADDING, 0.0))
                .intersect(ui.clip_rect()),
        )
        .text(
            layout.body.center(),
            egui::Align2::CENTER_CENTER,
            t!("tab.new"),
            egui::TextStyle::Button.resolve(ui.style()),
            style.text_color,
        );
}

fn paint_close_button(
    ui: &egui::Ui,
    layout: &TabLayout,
    style: &TabStyle,
    interaction: &TabInteraction,
) {
    let hovered = &ui.visuals().widgets.hovered;

    let color = if interaction.close_highlighted() {
        ui.painter().rect_filled(
            layout.close.shrink(CLOSE_PADDING),
            style.corner_radius,
            hovered.bg_fill,
        );
        hovered.fg_stroke.color
    } else {
        style.text_color
    };

    ui.painter().text(
        layout.close.center(),
        egui::Align2::CENTER_CENTER,
        CLOSE_SYMBOL,
        egui::TextStyle::Button.resolve(ui.style()),
        color,
    );
}
