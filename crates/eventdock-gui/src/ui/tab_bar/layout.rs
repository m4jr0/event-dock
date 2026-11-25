const PREFERRED_WIDTH: f32 = 134.0;
const MIN_WIDTH: f32 = 90.0;
pub(super) const CONTROL_WIDTH: f32 = 24.0;

pub(super) struct StripLayout {
    pub(super) tab_width: f32,
    pub(super) viewport_width: f32,
    pub(super) overflow: bool,
    pub(super) max_offset: f32,
}

impl StripLayout {
    pub(super) fn calculate(
        width: f32,
        count: usize,
        gap: f32,
    ) -> Self {
        let count = count as f32;
        let available = (width - CONTROL_WIDTH - count * gap).max(0.0);
        let fitting_width = available / count.max(1.0);
        let overflow = count > 0.0 && fitting_width < MIN_WIDTH;
        let tab_width = fitting_width.clamp(MIN_WIDTH, PREFERRED_WIDTH);
        let content = count * tab_width + (count - 1.0).max(0.0) * gap;

        let viewport_width = if overflow {
            (width - 3.0 * (CONTROL_WIDTH + gap)).max(0.0)
        } else {
            content
        };

        Self {
            tab_width,
            viewport_width,
            overflow,
            max_offset: (content - viewport_width).max(0.0),
        }
    }

    pub(super) fn reveal(
        &self,
        offset: f32,
        index: usize,
        gap: f32,
    ) -> f32 {
        let left = index as f32 * (self.tab_width + gap);
        let right = left + self.tab_width;

        let offset = if left < offset {
            left
        } else if right > offset + self.viewport_width {
            right - self.viewport_width
        } else {
            offset
        };

        offset.clamp(0.0, self.max_offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferred_width_when_roomy() {
        let layout = StripLayout::calculate(600.0, 2, 4.0);

        assert_eq!(layout.tab_width, PREFERRED_WIDTH);
        assert!(!layout.overflow);
    }

    #[test]
    fn compresses_before_overflow() {
        let layout = StripLayout::calculate(340.0, 3, 4.0);

        assert!(!layout.overflow);
        assert!(layout.tab_width < PREFERRED_WIDTH);
        assert!(layout.tab_width >= MIN_WIDTH);
    }

    #[test]
    fn overflow_reserves_controls() {
        let layout = StripLayout::calculate(340.0, 10, 4.0);

        assert!(layout.overflow);
        assert_eq!(layout.tab_width, MIN_WIDTH);
        assert_eq!(layout.viewport_width, 256.0);
        assert_eq!(layout.reveal(0.0, 9, 4.0), layout.max_offset);
    }
    #[test]
    fn empty_strip_does_not_overflow() {
        let layout = StripLayout::calculate(10.0, 0, 4.0);

        assert!(!layout.overflow);
        assert_eq!(layout.max_offset, 0.0);
    }
}
