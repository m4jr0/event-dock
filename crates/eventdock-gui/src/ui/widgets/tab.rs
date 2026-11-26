use crate::action::TabAction;
use crate::tab::{Tab, TabId};
use eframe::egui;
use rust_i18n::t;

const CLOSE_WIDTH: f32 = 22.0;
const TITLE_PADDING: f32 = 4.0;
const CLOSE_PADDING: f32 = 2.0;
const CLOSE_SYMBOL: &str = "×";

pub(crate) struct TabOutput {
    pub(crate) action: Option<TabAction>,
    pub(crate) drag_start: Option<f32>,
}

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
    ) -> TabOutput {
        let layout = TabLayout::allocate(ui, width);
        let interaction = TabInteraction::detect(ui, &layout);
        let style = TabStyle::resolve(ui, self.selected, &interaction);

        paint_background(ui.painter(), &layout, &style);
        paint_title(ui, ui.painter(), &layout, &style, self.tab.id());
        paint_close_button(ui, &layout, &style, &interaction);

        let drag_start = if interaction
            .body
            .drag_started_by(egui::PointerButton::Primary)
        {
            interaction
                .body
                .interact_pointer_pos()
                .map(|pos| {
                    pos.x
                        - interaction.body.drag_delta().x
                        - layout.bounds.left()
                })
        } else {
            None
        };

        TabOutput {
            action: interaction.action(self.tab.id()),
            drag_start,
        }
    }
}

pub(crate) fn paint_drag_preview(
    ui: &egui::Ui,
    bounds: egui::Rect,
    id: TabId,
    selected: bool,
) {
    paint_tab_preview(ui, ui.painter(), bounds, id, selected);
}

pub(crate) fn paint_tab_preview(
    ui: &egui::Ui,
    painter: &egui::Painter,
    bounds: egui::Rect,
    id: TabId,
    selected: bool,
) {
    let layout = TabLayout::from_bounds(bounds);
    let visuals = ui.visuals();
    let widget = if selected {
        &visuals.widgets.active
    } else {
        &visuals.widgets.inactive
    };

    let style = TabStyle {
        fill: if selected {
            visuals.selection.bg_fill
        } else {
            widget.bg_fill
        },
        stroke: if selected {
            visuals.selection.stroke
        } else {
            widget.bg_stroke
        },
        text_color: if selected {
            visuals.selection.stroke.color
        } else {
            widget.fg_stroke.color
        },
        corner_radius: widget.corner_radius,
    };

    paint_background(painter, &layout, &style);
    paint_title(ui, painter, &layout, &style, id);

    painter.text(
        layout.close.center(),
        egui::Align2::CENTER_CENTER,
        CLOSE_SYMBOL,
        egui::TextStyle::Button.resolve(ui.style()),
        style.text_color,
    );
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

        Self::from_bounds(bounds)
    }

    fn from_bounds(bounds: egui::Rect) -> Self {
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

        let body = ui.interact(
            layout.body,
            id.with("body"),
            egui::Sense::click_and_drag(),
        );

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
    painter: &egui::Painter,
    layout: &TabLayout,
    style: &TabStyle,
) {
    painter.rect(
        layout.bounds,
        style.corner_radius,
        style.fill,
        style.stroke,
        egui::StrokeKind::Inside,
    );
}

fn paint_title(
    ui: &egui::Ui,
    painter: &egui::Painter,
    layout: &TabLayout,
    style: &TabStyle,
    _id: TabId,
) {
    let title = t!("tab.new");
    #[cfg(debug_assertions)]
    let title = format!("{} #{}", title, _id.debug_index());

    let bounds = layout
        .body
        .shrink2(egui::vec2(TITLE_PADDING, 0.0));

    let galley = painter.layout_no_wrap(
        title.to_string(),
        egui::TextStyle::Button.resolve(ui.style()),
        style.text_color,
    );

    let position = title_position(bounds, galley.size());

    painter
        .with_clip_rect(bounds.intersect(painter.clip_rect()))
        .galley(position, galley, style.text_color);
}

fn title_position(
    bounds: egui::Rect,
    size: egui::Vec2,
) -> egui::Pos2 {
    egui::pos2(
        bounds.left() + ((bounds.width() - size.x) * 0.5).max(0.0),
        bounds.center().y - size.y * 0.5,
    )
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
