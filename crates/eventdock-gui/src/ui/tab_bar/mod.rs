mod layout;

use crate::action::Action;
use crate::tab::{TabId, TabManager};
use crate::ui::widgets::tab::TabWidget;
use eframe::egui;
use layout::{CONTROL_BUTTON_WIDTH, StripLayout};

const SCROLL_LEFT_SYMBOL: &str = "‹";
const SCROLL_RIGHT_SYMBOL: &str = "›";
const NEW_TAB_SYMBOL: &str = "+";

const SCROLL_DURATION: f32 = 0.15;
const SCROLL_EPSILON: f32 = 0.1;

#[derive(Default)]
pub(crate) struct TabBar {
    offset: f32,
    target_offset: f32,
    previous_count: usize,
    pointer_close_pending: bool,
    closing_layout_count: Option<usize>,
    previous_active: Option<TabId>,
    previous_viewport: f32,
}

impl TabBar {
    pub(crate) fn on_pointer_close(&mut self) {
        self.pointer_close_pending = true;
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        tabs: &TabManager,
    ) -> Option<Action> {
        let gap = ui.spacing().item_spacing.x;

        let count = tabs.tabs().len();
        let width = ui.available_width();

        let strip = egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(width, ui.spacing().interact_size.y),
        );

        let closing = count < self.previous_count;
        let pointer_close = std::mem::take(&mut self.pointer_close_pending);
        let hovering = ui.rect_contains_pointer(strip);
        self.update_closing_layout(count, closing, pointer_close, hovering);

        let layout = StripLayout::calculate(
            width,
            self.closing_layout_count
                .unwrap_or(count),
            gap,
        );

        self.update_offset(tabs, &layout, gap);
        self.animate_offset(ui.ctx());

        ui.horizontal(|ui| {
            if layout.overflow {
                self.show_overflow(ui, tabs, &layout, gap)
            } else {
                show_tabs(ui, tabs, layout.tab_width)
                    .or(show_new_tab_button(ui))
                    .or(show_empty_area(ui))
            }
        })
        .inner
    }

    fn update_closing_layout(
        &mut self,
        count: usize,
        closing: bool,
        pointer_close: bool,
        hovering: bool,
    ) {
        if count == 0 || count > self.previous_count {
            self.closing_layout_count = None;
        } else if closing && pointer_close {
            self.closing_layout_count = Some(
                self.closing_layout_count
                    .unwrap_or(self.previous_count),
            );

            self.target_offset = self.offset;
        } else if closing || !hovering {
            self.closing_layout_count = None;
        }
    }

    fn update_offset(
        &mut self,
        tabs: &TabManager,
        layout: &StripLayout,
        gap: f32,
    ) {
        let active = tabs.active_tab_id();
        let count = tabs.tabs().len();
        let viewport_changed = layout.viewport_width != self.previous_viewport;
        let count_changed = count != self.previous_count;

        self.target_offset = self
            .target_offset
            .clamp(0.0, layout.max_offset);

        let active_changed = active != self.previous_active;
        let layout_changed = viewport_changed || count_changed;

        let should_reveal = active_changed
            || (self.closing_layout_count.is_none() && layout_changed);

        if should_reveal
            && let Some(index) = tabs
                .tabs()
                .iter()
                .position(|tab| Some(tab.id()) == active)
        {
            self.target_offset = layout.offset_for_tab_visibility(
                self.target_offset,
                index,
                gap,
            );
        }

        if !layout.overflow {
            self.offset = 0.0;
        } else if viewport_changed {
            self.offset = self
                .offset
                .clamp(0.0, layout.max_offset);
        }

        self.previous_active = active;
        self.previous_viewport = layout.viewport_width;
        self.previous_count = count;
    }

    fn animate_offset(
        &mut self,
        ctx: &egui::Context,
    ) {
        let distance = self.target_offset - self.offset;

        if distance.abs() <= SCROLL_EPSILON {
            self.offset = self.target_offset;
            return;
        }

        let dt = ctx
            .input(|input| input.stable_dt)
            .clamp(0.0, SCROLL_DURATION);

        let blend = 1.0 - (-5.0 * dt / SCROLL_DURATION).exp();
        self.offset += distance * blend;
        ctx.request_repaint();
    }

    fn show_overflow(
        &mut self,
        ui: &mut egui::Ui,
        tabs: &TabManager,
        layout: &StripLayout,
        gap: f32,
    ) -> Option<Action> {
        let step = layout.tab_width + gap;

        if navigation_button(ui, SCROLL_LEFT_SYMBOL, self.target_offset > 0.0) {
            self.closing_layout_count = None;
            self.target_offset = (self.target_offset - step).max(0.0);
            ui.ctx().request_repaint();
        }

        let action = self.show_viewport(ui, tabs, layout, gap);

        if navigation_button(
            ui,
            SCROLL_RIGHT_SYMBOL,
            self.target_offset < layout.max_offset,
        ) {
            self.closing_layout_count = None;

            self.target_offset =
                (self.target_offset + step).min(layout.max_offset);

            ui.ctx().request_repaint();
        }

        action.or(show_new_tab_button(ui))
    }

    fn show_viewport(
        &mut self,
        ui: &mut egui::Ui,
        tabs: &TabManager,
        layout: &StripLayout,
        gap: f32,
    ) -> Option<Action> {
        let height = ui.spacing().interact_size.y;

        let (viewport, _) = ui.allocate_exact_size(
            egui::vec2(layout.viewport_width, height),
            egui::Sense::hover(),
        );

        if ui.rect_contains_pointer(viewport) {
            let delta = ui.input_mut(|input| {
                let delta = input.smooth_scroll_delta;
                input.smooth_scroll_delta = egui::Vec2::ZERO;
                delta
            });

            if delta != egui::Vec2::ZERO {
                self.closing_layout_count = None;
                self.target_offset = (self.target_offset - delta.x - delta.y)
                    .clamp(0.0, layout.max_offset);
                ui.ctx().request_repaint();
            }
        }

        let stride = layout.tab_width + gap;
        let first = (self.offset / stride).floor() as usize;

        let end = (((self.offset + viewport.width()) / stride).ceil() as usize)
            .min(tabs.tabs().len());

        let mut action = None;

        let content_right = viewport.left()
            + (tabs.tabs().len() as f32 * stride - gap).max(0.0)
            - self.offset;

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
                action = Some(Action::OpenTab);
            }
        }

        for index in first..end {
            let tab = &tabs.tabs()[index];
            let left = viewport.left() + index as f32 * stride - self.offset;

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

            let selected = tabs.active_tab_id() == Some(tab.id());

            action = TabWidget::new(tab, selected)
                .show(&mut child, layout.tab_width)
                .or(action);
        }

        action
    }
}

fn show_tabs(
    ui: &mut egui::Ui,
    tabs: &TabManager,
    width: f32,
) -> Option<Action> {
    let mut action = None;

    for tab in tabs.tabs() {
        let selected = tabs.active_tab_id() == Some(tab.id());

        action = ui
            .push_id(tab.id(), |ui| {
                TabWidget::new(tab, selected).show(ui, width)
            })
            .inner
            .or(action);
    }

    action
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

fn show_new_tab_button(ui: &mut egui::Ui) -> Option<Action> {
    navigation_button(ui, NEW_TAB_SYMBOL, true).then_some(Action::OpenTab)
}

fn show_empty_area(ui: &mut egui::Ui) -> Option<Action> {
    let size =
        egui::vec2(ui.available_width().max(0.0), ui.spacing().interact_size.y);

    let (_, response) = ui.allocate_exact_size(size, egui::Sense::click());

    (response.double_clicked() || response.middle_clicked())
        .then_some(Action::OpenTab)
}
