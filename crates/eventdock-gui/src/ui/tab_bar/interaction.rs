#[derive(Default)]
pub(super) struct InteractionState {
    pointer_close_pending: bool,
    closing_layout_count: Option<usize>,
}

impl InteractionState {
    pub(super) fn on_pointer_close(&mut self) {
        self.pointer_close_pending = true;
    }

    // Return whether the current scroll animation should stop.
    pub(super) fn update(
        &mut self,
        count: usize,
        previous_count: usize,
        hovering: bool,
    ) -> bool {
        let pointer_close = std::mem::take(&mut self.pointer_close_pending);
        let closing = count < previous_count;

        if count == 0 || count > previous_count {
            self.clear();
        } else if closing && pointer_close {
            self.closing_layout_count = Some(
                self.closing_layout_count
                    .unwrap_or(previous_count),
            );

            return true;
        } else if closing || !hovering {
            self.clear();
        }

        false
    }

    pub(super) fn layout_count(
        &self,
        count: usize,
    ) -> usize {
        self.closing_layout_count
            .unwrap_or(count)
    }

    pub(super) fn is_frozen(&self) -> bool {
        self.closing_layout_count.is_some()
    }

    pub(super) fn clear(&mut self) {
        self.closing_layout_count = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_pointer_closures_preserve_original_count() {
        let mut state = InteractionState::default();

        state.on_pointer_close();
        assert!(state.update(5, 6, true));

        state.on_pointer_close();
        assert!(state.update(4, 5, true));
        assert_eq!(state.layout_count(4), 6);
        assert!(!state.update(4, 4, false));
        assert!(!state.is_frozen());
    }

    #[test]
    fn keyboard_closure_clears_freeze() {
        let mut state = InteractionState::default();
        state.on_pointer_close();
        state.update(5, 6, true);

        assert!(!state.update(4, 5, true));
        assert_eq!(state.layout_count(4), 4);
    }

    #[test]
    fn pointer_notification_is_consumed_once() {
        let mut state = InteractionState::default();
        state.on_pointer_close();

        assert!(!state.update(6, 6, true));
        assert!(!state.update(5, 6, true));
        assert!(!state.is_frozen());
    }

    #[test]
    fn empty_strip_clears_freeze() {
        let mut state = InteractionState::default();
        state.on_pointer_close();
        state.update(1, 2, true);
        state.on_pointer_close();

        assert!(!state.update(0, 1, true));
        assert!(!state.is_frozen());
    }
}
