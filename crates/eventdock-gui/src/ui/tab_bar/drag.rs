use super::layout::StripLayout;
use crate::action::TabAction;
use crate::tab::{TabId, TabManager};
use crate::ui::animation::exponential_blend;
use crate::ui::widgets::tab::{paint_drag_preview, paint_tab_preview};
use eframe::egui;
use std::collections::HashMap;

const EDGE_SCROLL_ZONE: f32 = 48.0;
const EDGE_SCROLL_SPEED: f32 = 600.0;

#[derive(Clone, Copy)]
struct DragState {
    id: TabId,
    grab_offset: f32,
}

#[derive(Default)]
pub(super) struct TabDrag {
    state: Option<DragState>,
    preview_offsets: HashMap<TabId, f32>,
}

impl TabDrag {
    pub(super) fn is_active(&self) -> bool {
        self.state.is_some()
    }

    pub(super) fn start(
        &mut self,
        id: TabId,
        grab_offset: f32,
        ctx: &egui::Context,
    ) {
        self.preview_offsets.clear();
        self.state = Some(DragState { id, grab_offset });
        ctx.request_repaint();
    }

    pub(super) fn edge_scroll_delta(
        &self,
        ui: &egui::Ui,
        viewport: egui::Rect,
    ) -> f32 {
        if !self.is_active() {
            return 0.0;
        }

        ui.input(|input| {
            if !input.pointer.primary_down()
                || input
                    .pointer
                    .button_released(egui::PointerButton::Primary)
                || input.key_pressed(egui::Key::Escape)
                || !input.focused
            {
                return 0.0;
            }

            let Some(pointer) = input.pointer.interact_pos() else {
                return 0.0;
            };

            if pointer.y < viewport.top() || pointer.y > viewport.bottom() {
                return 0.0;
            }

            edge_scroll_velocity(pointer.x, viewport.left(), viewport.right())
                * input.stable_dt.clamp(0.0, 0.05)
        })
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        tabs: &TabManager,
        layout: &StripLayout,
        viewport: egui::Rect,
        offset: f32,
    ) -> Option<TabAction> {
        let drag = self.state?;
        let action = self.finish(ui, tabs, layout, viewport.left(), offset);

        let pointer = ui.input(|input| input.pointer.interact_pos());

        let Some(source) = tabs
            .tabs()
            .iter()
            .position(|tab| tab.id() == drag.id)
        else {
            self.preview_offsets.clear();
            return action;
        };

        let dragged_left = pointer.map_or(layout.tab_left(source), |pos| {
            pos.x - viewport.left() + offset - drag.grab_offset
        });

        let destination = layout.drag_destination(
            dragged_left + layout.tab_width * 0.5,
            tabs.tabs().len(),
        );

        let dt = ui
            .input(|input| input.stable_dt)
            .clamp(0.0, 0.1);

        let blend = exponential_blend(30.0, dt);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(viewport));
        child.set_clip_rect(viewport.intersect(ui.clip_rect()));

        self.paint_neighbors(
            &child,
            tabs,
            layout,
            viewport,
            offset,
            source,
            destination,
            blend,
        );

        paint_dragged_tab(ui, layout, viewport, offset, dragged_left, drag.id);

        if self.state.is_some() {
            ui.ctx().request_repaint();
        } else {
            self.preview_offsets.clear();
            ui.ctx().request_repaint();
        }

        action
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Keep preview geometry explicit"
    )]
    fn paint_neighbors(
        &mut self,
        ui: &egui::Ui,
        tabs: &TabManager,
        layout: &StripLayout,
        viewport: egui::Rect,
        offset: f32,
        source: usize,
        destination: usize,
        blend: f32,
    ) {
        for (index, tab) in tabs.tabs().iter().enumerate() {
            if index == source {
                continue;
            }

            let slot = layout.preview_slot(index, source, destination);
            let target = layout.tab_left(slot) - layout.tab_left(index);

            let shift = self
                .preview_offsets
                .entry(tab.id())
                .or_default();

            *shift += (target - *shift) * blend;

            let left =
                viewport.left() + layout.tab_left(index) - offset + *shift;

            let bounds = egui::Rect::from_min_size(
                egui::pos2(left, viewport.top()),
                egui::vec2(layout.tab_width, viewport.height()),
            );

            if bounds.intersects(viewport) {
                paint_drag_preview(ui, bounds, tab.id(), false);
            }
        }
    }

    pub(super) fn finish(
        &mut self,
        ui: &egui::Ui,
        tabs: &TabManager,
        layout: &StripLayout,
        left: f32,
        offset: f32,
    ) -> Option<TabAction> {
        let drag = self.state?;

        let (down, released, pointer, cancel) = ui.input(|input| {
            (
                input.pointer.primary_down(),
                input
                    .pointer
                    .button_released(egui::PointerButton::Primary),
                input.pointer.interact_pos(),
                input.key_pressed(egui::Key::Escape) || !input.focused,
            )
        });

        if cancel || !contains_tab(tabs, drag.id) {
            self.state = None;
            return None;
        }

        if down && !released {
            return None;
        }

        self.state = None;

        if !released {
            return None;
        }

        let center = pointer?.x - left + offset - drag.grab_offset
            + layout.tab_width * 0.5;

        let destination = layout.drag_destination(center, tabs.tabs().len());

        Some(TabAction::Move(drag.id, destination))
    }
}

fn paint_dragged_tab(
    ui: &egui::Ui,
    layout: &StripLayout,
    viewport: egui::Rect,
    offset: f32,
    dragged_left: f32,
    id: TabId,
) {
    let left = viewport.left() + dragged_left - offset;

    let bounds = egui::Rect::from_min_size(
        egui::pos2(left, viewport.top()),
        egui::vec2(layout.tab_width, viewport.height()),
    );

    let painter = foreground_painter(ui);
    let bounds = clamp_horizontal(bounds, painter.clip_rect());

    paint_tab_preview(ui, &painter, bounds, id, true);
}

fn edge_scroll_velocity(
    x: f32,
    left: f32,
    right: f32,
) -> f32 {
    let zone = EDGE_SCROLL_ZONE.min((right - left) * 0.5);

    if zone <= 0.0 {
        return 0.0;
    }

    let left_strength = ((left + zone - x) / zone).clamp(0.0, 1.0);
    let right_strength = ((x - right + zone) / zone).clamp(0.0, 1.0);
    (right_strength - left_strength) * EDGE_SCROLL_SPEED
}

fn foreground_painter(ui: &egui::Ui) -> egui::Painter {
    let layer = egui::LayerId::new(
        egui::Order::Foreground,
        ui.scope_id().with("dragged_tab"),
    );

    ui.ctx().layer_painter(layer)
}

fn clamp_horizontal(
    bounds: egui::Rect,
    clip: egui::Rect,
) -> egui::Rect {
    let left = bounds.left().clamp(
        clip.left(),
        (clip.right() - bounds.width()).max(clip.left()),
    );

    bounds.translate(egui::vec2(left - bounds.left(), 0.0))
}

fn contains_tab(
    tabs: &TabManager,
    id: TabId,
) -> bool {
    tabs.tabs()
        .iter()
        .any(|tab| tab.id() == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_velocity_is_directional_and_bounded() {
        assert_eq!(edge_scroll_velocity(0.0, 0.0, 200.0), -EDGE_SCROLL_SPEED);
        assert_eq!(edge_scroll_velocity(200.0, 0.0, 200.0), EDGE_SCROLL_SPEED);
        assert_eq!(edge_scroll_velocity(100.0, 0.0, 200.0), 0.0);
        assert_eq!(
            edge_scroll_velocity(24.0, 0.0, 200.0),
            -EDGE_SCROLL_SPEED * 0.5
        );
        assert_eq!(edge_scroll_velocity(300.0, 0.0, 200.0), EDGE_SCROLL_SPEED);
    }

    #[test]
    fn narrow_viewports_have_nonoverlapping_zones() {
        assert_eq!(edge_scroll_velocity(10.0, 0.0, 20.0), 0.0);
        assert_eq!(edge_scroll_velocity(0.0, 0.0, 0.0), 0.0);
    }
}
