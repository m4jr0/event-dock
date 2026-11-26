const PREFERRED_WIDTH: f32 = 134.0;
const MIN_WIDTH: f32 = 90.0;
pub(super) const CONTROL_BUTTON_WIDTH: f32 = 24.0;

use std::ops::Range;

pub(super) struct StripLayout {
    gap: f32,
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
        let count_f32 = count as f32;

        let available =
            (width - CONTROL_BUTTON_WIDTH - count_f32 * gap).max(0.0);

        let fitting_width = available / count.max(1) as f32;
        let overflow = count > 0 && fitting_width < MIN_WIDTH;
        let tab_width = fitting_width.clamp(MIN_WIDTH, PREFERRED_WIDTH);

        let content =
            count_f32 * tab_width + count.saturating_sub(1) as f32 * gap;

        let viewport_width = if overflow {
            (width - 3.0 * (CONTROL_BUTTON_WIDTH + gap)).max(0.0)
        } else {
            content
        };

        Self {
            gap,
            tab_width,
            viewport_width,
            overflow,
            max_offset: (content - viewport_width).max(0.0),
        }
    }

    pub(super) fn stride(&self) -> f32 {
        self.tab_width + self.gap
    }

    pub(super) fn tab_left(
        &self,
        index: usize,
    ) -> f32 {
        index as f32 * self.stride()
    }

    pub(super) fn content_width(
        &self,
        count: usize,
    ) -> f32 {
        (count as f32 * self.stride() - self.gap).max(0.0)
    }

    pub(super) fn visible_range(
        &self,
        offset: f32,
        viewport_width: f32,
        count: usize,
    ) -> Range<usize> {
        let first = ((offset / self.stride()).floor() as usize).min(count);

        let end = (((offset + viewport_width) / self.stride()).ceil() as usize)
            .min(count);

        first..end.max(first)
    }

    pub(super) fn offset_for_tab_visibility(
        &self,
        offset: f32,
        index: usize,
    ) -> f32 {
        let left = self.tab_left(index);
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
        assert_eq!(layout.offset_for_tab_visibility(0.0, 9), layout.max_offset);
    }
    #[test]
    fn tab_geometry() {
        let layout = StripLayout::calculate(340.0, 10, 4.0);

        assert_eq!(layout.stride(), 94.0);
        assert_eq!(layout.tab_left(2), 188.0);
        assert_eq!(layout.content_width(0), 0.0);
        assert_eq!(layout.content_width(3), 278.0);
    }

    #[test]
    fn visible_range_includes_partial_tabs() {
        let layout = StripLayout::calculate(340.0, 10, 4.0);

        assert_eq!(layout.visible_range(100.0, 256.0, 10), 1..4);
        assert_eq!(layout.visible_range(0.0, 188.0, 10), 0..2);
    }

    #[test]
    fn visible_range_respects_actual_count() {
        let layout = StripLayout::calculate(340.0, 10, 4.0);

        assert_eq!(layout.visible_range(100.0, 256.0, 2), 1..2);
        assert_eq!(layout.visible_range(500.0, 256.0, 2), 2..2);
        assert_eq!(layout.visible_range(0.0, 256.0, 0), 0..0);
    }

    #[test]
    fn empty_strip_does_not_overflow() {
        let layout = StripLayout::calculate(10.0, 0, 4.0);

        assert!(!layout.overflow);
        assert_eq!(layout.max_offset, 0.0);
    }
}
