#[derive(Debug, Clone, Copy)]
pub(crate) struct Metrics {
    pub(crate) spacing_small: f32,
    pub(crate) spacing_medium: f32,
    pub(crate) spacing_large: f32,
    pub(crate) corner_radius: u8,
    pub(crate) border_width: f32,
    pub(crate) foreground_stroke_width: f32,
    pub(crate) control_height: f32,
    #[allow(dead_code)] // Used by the future log table.
    pub(crate) row_height: f32,
}
