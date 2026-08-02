use super::{Metrics, Theme, Typography};
use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, TextStyle};

pub(super) fn build(
    theme: &Theme,
    dark: bool,
) -> egui::Style {
    let mut style = egui::Style {
        visuals: if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        },
        ..Default::default()
    };

    apply_spacing(&mut style.spacing, &theme.metrics);
    apply_typography(&mut style, &theme.typography);
    apply_visuals(&mut style.visuals, theme);
    apply_widget_visuals(&mut style.visuals.widgets, theme);

    style
}

fn apply_spacing(
    spacing: &mut egui::Spacing,
    m: &Metrics,
) {
    spacing.item_spacing = egui::vec2(m.spacing_medium, m.spacing_small);
    spacing.button_padding = egui::vec2(m.spacing_medium, m.spacing_small);
    spacing.interact_size.y = m.control_height;
    spacing.indent = m.spacing_large;
}

fn apply_typography(
    style: &mut egui::Style,
    t: &Typography,
) {
    for (role, font) in [
        (TextStyle::Small, FontId::proportional(t.small)),
        (TextStyle::Body, FontId::proportional(t.body)),
        (TextStyle::Button, FontId::proportional(t.body)),
        (TextStyle::Heading, FontId::proportional(t.heading)),
        (TextStyle::Monospace, FontId::monospace(t.monospace)),
    ] {
        style.text_styles.insert(role, font);
    }
}

fn apply_visuals(
    visuals: &mut egui::Visuals,
    theme: &Theme,
) {
    let p = &theme.palette;
    let m = &theme.metrics;

    visuals.panel_fill = p.surface.background;
    visuals.window_fill = p.surface.panel;
    visuals.window_stroke = Stroke::new(m.border_width, p.surface.border);

    visuals.window_corner_radius = CornerRadius::same(m.corner_radius);

    visuals.menu_corner_radius = CornerRadius::same(m.corner_radius);

    visuals.extreme_bg_color = p.surface.background;
    visuals.faint_bg_color = p.surface.raised;
    visuals.code_bg_color = p.surface.raised;

    visuals.hyperlink_color = p.interaction.accent;
    visuals.warn_fg_color = p.status.warning;
    visuals.error_fg_color = p.status.error;

    visuals.selection.bg_fill = p.interaction.selected;
    visuals.selection.stroke = Stroke::new(m.border_width, p.interaction.focus);

    visuals.override_text_color = None;

    // Avoid inheriting preset-specific shadow colors.
    visuals.window_shadow.color = p.surface.transparent;
    visuals.popup_shadow.color = p.surface.transparent;
}

fn apply_widget_visuals(
    widgets: &mut egui::style::Widgets,
    theme: &Theme,
) {
    let p = &theme.palette;

    for (widget, fill, text, border) in [
        (
            &mut widgets.noninteractive,
            p.surface.panel,
            p.text.secondary,
            p.surface.border,
        ),
        (
            &mut widgets.inactive,
            p.surface.raised,
            p.text.primary,
            p.surface.border,
        ),
        (
            &mut widgets.hovered,
            p.interaction.hovered,
            p.interaction.accent_hovered,
            p.interaction.focus,
        ),
        (
            &mut widgets.active,
            p.interaction.pressed,
            p.interaction.accent_pressed,
            p.interaction.focus,
        ),
        (
            &mut widgets.open,
            p.interaction.selected,
            p.text.primary,
            p.surface.border,
        ),
    ] {
        apply_widget_style(widget, fill, text, border, &theme.metrics);
    }
}

fn apply_widget_style(
    widget: &mut egui::style::WidgetVisuals,
    fill: Color32,
    text: Color32,
    border: Color32,
    metrics: &Metrics,
) {
    widget.bg_fill = fill;
    widget.weak_bg_fill = fill;
    widget.bg_stroke = Stroke::new(metrics.border_width, border);
    widget.fg_stroke = Stroke::new(metrics.foreground_stroke_width, text);
    widget.corner_radius = CornerRadius::same(metrics.corner_radius);
}
