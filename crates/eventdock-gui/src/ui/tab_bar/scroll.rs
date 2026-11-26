const SCROLL_DURATION: f32 = 0.15;
const SCROLL_EPSILON: f32 = 0.1;

#[derive(Default)]
pub(super) struct ScrollState {
    offset: f32,
    target_offset: f32,
}

impl ScrollState {
    pub(super) fn offset(&self) -> f32 {
        self.offset
    }

    pub(super) fn target_offset(&self) -> f32 {
        self.target_offset
    }

    pub(super) fn set_target(
        &mut self,
        offset: f32,
    ) {
        self.target_offset = offset;
    }

    pub(super) fn clamp_target(
        &mut self,
        max_offset: f32,
    ) {
        self.target_offset = self
            .target_offset
            .clamp(0.0, max_offset);
    }

    pub(super) fn clamp_offset(
        &mut self,
        max_offset: f32,
    ) {
        self.offset = self.offset.clamp(0.0, max_offset);
    }

    pub(super) fn stop(&mut self) {
        self.target_offset = self.offset;
    }

    pub(super) fn scroll_by(
        &mut self,
        delta: f32,
        max_offset: f32,
    ) {
        self.target_offset =
            (self.target_offset + delta).clamp(0.0, max_offset);
    }

    // Return whether another UI pass is needed.
    pub(super) fn animate(
        &mut self,
        dt: f32,
    ) -> bool {
        let distance = self.target_offset - self.offset;

        if distance.abs() <= SCROLL_EPSILON {
            self.offset = self.target_offset;
            return false;
        }

        let dt = dt.clamp(0.0, SCROLL_DURATION);
        let blend = 1.0 - (-5.0 * dt / SCROLL_DURATION).exp();
        self.offset += distance * blend;

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_is_bounded() {
        let mut state = ScrollState::default();

        state.scroll_by(200.0, 100.0);
        assert_eq!(state.target_offset(), 100.0);

        state.scroll_by(-200.0, 100.0);
        assert_eq!(state.target_offset(), 0.0);
    }

    #[test]
    fn animation_approaches_target_and_stops() {
        let mut state = ScrollState::default();

        state.set_target(100.0);
        assert!(state.animate(0.05));
        assert!(state.offset() > 0.0 && state.offset() < 100.0);

        for _ in 0..100 {
            state.animate(0.016);
        }

        assert_eq!(state.offset(), 100.0);
        assert!(!state.animate(0.016));
    }

    #[test]
    fn stop_preserves_current_position() {
        let mut state = ScrollState::default();
        state.set_target(100.0);
        state.animate(0.05);
        state.stop();

        assert_eq!(state.target_offset(), state.offset());
        assert!(!state.animate(0.016));
    }
}
