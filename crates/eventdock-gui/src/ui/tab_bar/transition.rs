use super::layout::StripLayout;
use crate::tab::{TabId, TabManager};
use crate::ui::animation::exponential_blend;
use std::collections::HashMap;

const RATE: f32 = 30.0;
const EPSILON: f32 = 0.1;

#[derive(Default)]
pub(super) struct TabTransition {
    offsets: HashMap<TabId, f32>,
    positions: HashMap<TabId, f32>,
    next_positions: HashMap<TabId, f32>,
}

impl TabTransition {
    pub(super) fn clear(&mut self) {
        self.offsets.clear();
        self.positions.clear();
        self.next_positions.clear();
    }

    pub(super) fn offset(
        &self,
        id: TabId,
    ) -> f32 {
        self.offsets
            .get(&id)
            .copied()
            .unwrap_or_default()
    }

    pub(super) fn is_moving(&self) -> bool {
        !self.offsets.is_empty()
    }

    pub(super) fn update_positions(
        &mut self,
        tabs: &TabManager,
        layout: &StripLayout,
        closing: bool,
        dt: f32,
    ) -> bool {
        let blend = blend(dt);
        self.next_positions.clear();

        for (index, tab) in tabs.tabs().iter().enumerate() {
            let id = tab.id();
            let target = layout.tab_left(index);

            let previous = self
                .positions
                .get(&id)
                .copied()
                .unwrap_or(target);

            let offset =
                self.offset(id) + if closing { previous - target } else { 0.0 };

            self.set_offset(id, offset * (1.0 - blend));
            self.next_positions.insert(id, target);
        }

        std::mem::swap(&mut self.positions, &mut self.next_positions);

        self.offsets
            .retain(|id, _| self.positions.contains_key(id));

        self.is_moving()
    }

    pub(super) fn update_drag(
        &mut self,
        tabs: &TabManager,
        layout: &StripLayout,
        source: usize,
        destination: usize,
        dt: f32,
    ) {
        let blend = blend(dt);

        for (index, tab) in tabs.tabs().iter().enumerate() {
            if index == source {
                continue;
            }

            let slot = layout.preview_slot(index, source, destination);
            let target = layout.tab_left(slot) - layout.tab_left(index);
            let current = self.offset(tab.id());
            self.set_offset(tab.id(), current + (target - current) * blend);
        }
    }

    fn set_offset(
        &mut self,
        id: TabId,
        offset: f32,
    ) {
        if offset.abs() <= EPSILON {
            self.offsets.remove(&id);
        } else {
            self.offsets.insert(id, offset);
        }
    }
}

fn blend(dt: f32) -> f32 {
    exponential_blend(RATE, dt.clamp(0.0, 0.1))
}
