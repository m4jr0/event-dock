mod drag;
mod interaction;
mod layout;
mod scroll;

use crate::action::TabAction;
use crate::tab::{Tab, TabId, TabManager};
use crate::ui::widgets::tab::TabWidget;
use drag::TabDrag;
use eframe::egui;
use interaction::InteractionState;
use layout::{CONTROL_BUTTON_WIDTH, StripLayout};
use scroll::ScrollState;

const SCROLL_LEFT_SYMBOL: &str = "‹";
const SCROLL_RIGHT_SYMBOL: &str = "›";
const NEW_TAB_SYMBOL: &str = "+";

#[derive(Default)]
pub(crate) struct TabBar {
    scroll: ScrollState,
    interaction: InteractionState,
    previous: LayoutSnapshot,
    drag: TabDrag,
}

#[derive(Default)]
struct LayoutSnapshot {
    count: usize,
    active: Option<TabId>,
    viewport_width: f32,
}

impl TabBar {
    pub(crate) fn on_pointer_close(&mut self) {
        self.interaction.on_pointer_close();
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        tabs: &TabManager,
    ) -> Option<TabAction> {
        let gap = ui.spacing().item_spacing.x;

        let count = tabs.tabs().len();
        let width = ui.available_width();

        let strip = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(width, ui.spacing().interact_size.y),
        );

        let hovering = ui.rect_contains_pointer(strip);

        if self
            .interaction
            .update(count, self.previous.count, hovering)
        {
            self.scroll.stop();
        }

        let layout = StripLayout::calculate(
            width,
            self.interaction.layout_count(count),
            gap,
        );

        self.update_offset(tabs, &layout);
        let dt = ui.ctx().input(|input| input.stable_dt);

        if self.scroll.animate(dt) {
            ui.ctx().request_repaint();
        }

        ui.horizontal(|ui| {
            if layout.overflow {
                self.show_overflow(ui, tabs, &layout)
            } else {
                self.show_tabs(ui, tabs, &layout)
                    .or(show_new_tab_button(ui))
                    .or(show_empty_area(ui))
            }
        })
        .inner
    }

    fn update_offset(
        &mut self,
        tabs: &TabManager,
        layout: &StripLayout,
    ) {
        let active = tabs.active_tab_id();
        let count = tabs.tabs().len();

        let viewport_changed =
            layout.viewport_width != self.previous.viewport_width;

        let count_changed = count != self.previous.count;

        self.scroll
            .clamp_target(layout.max_offset);

        let active_changed = active != self.previous.active;
        let layout_changed = viewport_changed || count_changed;

        let should_reveal =
            active_changed || (!self.interaction.is_frozen() && layout_changed);

        if should_reveal
            && !self.drag.is_active()
            && let Some(index) = tabs
                .tabs()
                .iter()
                .position(|tab| Some(tab.id()) == active)
        {
            self.scroll.set_target(
                layout.offset_for_tab_visibility(
                    self.scroll.target_offset(),
                    index,
                ),
            );
        }

        if !layout.overflow {
            self.scroll.clamp_offset(0.0);
        } else if viewport_changed {
            self.scroll
                .clamp_offset(layout.max_offset);
        }

        self.previous = LayoutSnapshot {
            active,
            count,
            viewport_width: layout.viewport_width,
        };
    }

    fn show_overflow(
        &mut self,
        ui: &mut egui::Ui,
        tabs: &TabManager,
        layout: &StripLayout,
    ) -> Option<TabAction> {
        let step = layout.stride();

        if navigation_button(
            ui,
            SCROLL_LEFT_SYMBOL,
            self.scroll.target_offset() > 0.0,
        ) {
            self.interaction.clear();

            self.scroll
                .scroll_by(-step, layout.max_offset);

            ui.ctx().request_repaint();
        }

        let action = self.show_viewport(ui, tabs, layout);

        if navigation_button(
            ui,
            SCROLL_RIGHT_SYMBOL,
            self.scroll.target_offset() < layout.max_offset,
        ) {
            self.interaction.clear();

            self.scroll
                .scroll_by(step, layout.max_offset);

            ui.ctx().request_repaint();
        }

        action.or(show_new_tab_button(ui))
    }

    fn show_viewport(
        &mut self,
        ui: &mut egui::Ui,
        tabs: &TabManager,
        layout: &StripLayout,
    ) -> Option<TabAction> {
        let height = ui.spacing().interact_size.y;

        let (viewport, _) = ui.allocate_exact_size(
            egui::vec2(layout.viewport_width, height),
            egui::Sense::hover(),
        );

        if self.drag.is_active() {
            self.handle_drag_scroll(ui, viewport, layout);
            return self.drag.show(
                ui,
                tabs,
                layout,
                viewport,
                self.scroll.offset(),
            );
        }

        self.handle_scroll_input(ui, viewport, layout);

        let visible = layout.visible_range(
            self.scroll.offset(),
            viewport.width(),
            tabs.tabs().len(),
        );

        let mut action =
            self.show_viewport_empty_area(ui, tabs, layout, viewport);

        for index in visible {
            action = self
                .show_viewport_tab(ui, tabs, layout, viewport, index)
                .or(action);
        }

        let offset = self.scroll.offset();

        self.drag
            .finish(ui, tabs, layout, viewport.left(), offset)
            .or(action)
    }

    fn show_tabs(
        &mut self,
        ui: &mut egui::Ui,
        tabs: &TabManager,
        layout: &StripLayout,
    ) -> Option<TabAction> {
        let left = ui.cursor().min.x;
        if self.drag.is_active() {
            let (viewport, _) = ui.allocate_exact_size(
                egui::vec2(
                    layout.content_width(tabs.tabs().len()),
                    ui.spacing().interact_size.y,
                ),
                egui::Sense::hover(),
            );
            return self
                .drag
                .show(ui, tabs, layout, viewport, 0.0);
        }
        let mut action = None;

        for tab in tabs.tabs() {
            action = ui
                .push_id(tab.id(), |ui| {
                    self.show_tab(
                        ui,
                        tab,
                        tabs.active_tab_id(),
                        layout.tab_width,
                    )
                })
                .inner
                .or(action);
        }

        self.drag
            .finish(ui, tabs, layout, left, 0.0)
            .or(action)
    }

    fn handle_drag_scroll(
        &mut self,
        ui: &egui::Ui,
        viewport: egui::Rect,
        layout: &StripLayout,
    ) {
        let delta = self
            .drag
            .edge_scroll_delta(ui, viewport);

        self.scroll
            .scroll_immediately_by(delta, layout.max_offset);
    }

    fn handle_scroll_input(
        &mut self,
        ui: &mut egui::Ui,
        viewport: egui::Rect,
        layout: &StripLayout,
    ) {
        if ui.rect_contains_pointer(viewport) {
            let delta = ui.input_mut(|input| {
                let delta = input.smooth_scroll_delta;
                input.smooth_scroll_delta = egui::Vec2::ZERO;

                delta
            });

            if delta != egui::Vec2::ZERO {
                self.interaction.clear();

                self.scroll.set_target(
                    (self.scroll.target_offset() - delta.x - delta.y)
                        .clamp(0.0, layout.max_offset),
                );

                ui.ctx().request_repaint();
            }
        }
    }

    fn show_viewport_empty_area(
        &self,
        ui: &mut egui::Ui,
        tabs: &TabManager,
        layout: &StripLayout,
        viewport: egui::Rect,
    ) -> Option<TabAction> {
        let mut action = None;

        let content_right = viewport.left()
            + layout.content_width(tabs.tabs().len())
            - self.scroll.offset();

        if content_right < viewport.right() {
            let empty = egui::Rect::from_min_max(
                egui::pos2(content_right.max(viewport.left()), viewport.top()),
                viewport.right_bottom(),
            );

            let response = ui.interact(
                empty,
                ui.scope_id().with("overflow_empty"),
                egui::Sense::click(),
            );

            if response.double_clicked() || response.middle_clicked() {
                action = Some(TabAction::Open);
            }
        }

        action
    }

    fn show_viewport_tab(
        &mut self,
        ui: &mut egui::Ui,
        tabs: &TabManager,
        layout: &StripLayout,
        viewport: egui::Rect,
        index: usize,
    ) -> Option<TabAction> {
        let tab = &tabs.tabs()[index];
        let height = viewport.height();

        let left =
            viewport.left() + layout.tab_left(index) - self.scroll.offset();

        let bounds = egui::Rect::from_min_size(
            egui::pos2(left, viewport.top()),
            egui::vec2(layout.tab_width, height),
        );

        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(tab.id())
                .max_rect(bounds)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );

        child.set_clip_rect(viewport.intersect(ui.clip_rect()));
        self.show_tab(&mut child, tab, tabs.active_tab_id(), layout.tab_width)
    }

    fn show_tab(
        &mut self,
        ui: &mut egui::Ui,
        tab: &Tab,
        active: Option<TabId>,
        width: f32,
    ) -> Option<TabAction> {
        let output =
            TabWidget::new(tab, active == Some(tab.id())).show(ui, width);
        if let Some(grab_offset) = output.drag_start {
            self.interaction.clear();
            self.scroll.stop();
            self.drag
                .start(tab.id(), grab_offset, ui.ctx());
            return Some(TabAction::Select(tab.id()));
        }

        output.action
    }
}

fn navigation_button(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
) -> bool {
    ui.add_enabled(
        enabled,
        egui::Button::new(label).min_size(egui::vec2(
            CONTROL_BUTTON_WIDTH,
            ui.spacing().interact_size.y,
        )),
    )
    .clicked()
}

fn show_new_tab_button(ui: &mut egui::Ui) -> Option<TabAction> {
    navigation_button(ui, NEW_TAB_SYMBOL, true).then_some(TabAction::Open)
}

fn show_empty_area(ui: &mut egui::Ui) -> Option<TabAction> {
    let size =
        egui::vec2(ui.available_width().max(0.0), ui.spacing().interact_size.y);

    let (_, response) = ui.allocate_exact_size(size, egui::Sense::click());

    (response.double_clicked() || response.middle_clicked())
        .then_some(TabAction::Open)
}
