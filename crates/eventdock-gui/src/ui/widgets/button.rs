use crate::ui::theme;
use eframe::egui;

#[allow(dead_code)] // Additional variants will be used by session views.
pub(crate) enum ButtonKind {
    Primary,
    Secondary,
    Danger,
    Ghost,
}

#[derive(Clone, Copy)]
struct ButtonColors {
    fill: egui::Color32,
    text: egui::Color32,
    border: egui::Color32,
}

struct ButtonStyle {
    normal: ButtonColors,
    hovered: ButtonColors,
    pressed: ButtonColors,
    disabled: ButtonColors,
}

pub(crate) fn button(
    ui: &mut egui::Ui,
    label: impl Into<egui::WidgetText>,
    kind: ButtonKind,
) -> egui::Response {
    ui.scope(|ui| {
        apply_style(ui, &kind);
        ui.button(label)
    })
    .inner
}

fn resolve_style(
    theme: &theme::Theme,
    kind: &ButtonKind,
) -> ButtonStyle {
    match kind {
        ButtonKind::Primary => primary_style(theme),
        ButtonKind::Secondary => secondary_style(theme),
        ButtonKind::Danger => danger_style(theme),
        ButtonKind::Ghost => ghost_style(theme),
    }
}

fn primary_style(theme: &theme::Theme) -> ButtonStyle {
    let p = &theme.palette;

    let normal = ButtonColors {
        fill: p.interaction.accent,
        text: p.text.on_accent,
        border: p.interaction.accent,
    };

    ButtonStyle {
        normal,

        hovered: ButtonColors {
            fill: p.interaction.accent_hovered,
            border: p.text.on_accent,
            ..normal
        },

        pressed: ButtonColors {
            fill: p.interaction.accent_pressed,
            border: p.text.on_accent,
            ..normal
        },

        disabled: disabled_colors(theme),
    }
}

fn secondary_style(theme: &theme::Theme) -> ButtonStyle {
    let p = &theme.palette;

    neutral_style(
        theme,
        ButtonColors {
            fill: p.surface.raised,
            text: p.text.primary,
            border: p.surface.border,
        },
    )
}

fn danger_style(theme: &theme::Theme) -> ButtonStyle {
    let p = &theme.palette;

    neutral_style(
        theme,
        ButtonColors {
            fill: p.surface.panel,
            text: p.status.error,
            border: p.status.error,
        },
    )
}

fn ghost_style(theme: &theme::Theme) -> ButtonStyle {
    let p = &theme.palette;

    neutral_style(
        theme,
        ButtonColors {
            fill: p.surface.transparent,
            text: p.text.primary,
            border: p.surface.transparent,
        },
    )
}

fn neutral_style(
    theme: &theme::Theme,
    normal: ButtonColors,
) -> ButtonStyle {
    let p = &theme.palette;

    ButtonStyle {
        normal,

        hovered: ButtonColors {
            fill: p.interaction.hovered,
            ..normal
        },

        pressed: ButtonColors {
            fill: p.interaction.pressed,
            ..normal
        },

        disabled: disabled_colors(theme),
    }
}

fn disabled_colors(theme: &theme::Theme) -> ButtonColors {
    let p = &theme.palette;

    ButtonColors {
        fill: p.surface.raised,
        text: p.text.disabled,
        border: p.surface.border,
    }
}

fn apply_style(
    ui: &mut egui::Ui,
    kind: &ButtonKind,
) {
    let theme = theme::current(ui);
    let style = resolve_style(theme, kind);
    let enabled = ui.is_enabled();
    let widgets = &mut ui.visuals_mut().widgets;

    for (widget, colors) in [
        (&mut widgets.noninteractive, style.normal),
        (&mut widgets.inactive, style.normal),
        (&mut widgets.hovered, style.hovered),
        (&mut widgets.active, style.pressed),
        (&mut widgets.open, style.pressed),
    ] {
        apply_colors(
            widget,
            if enabled { colors } else { style.disabled },
            theme,
        );
    }
}

fn apply_colors(
    widget: &mut egui::style::WidgetVisuals,
    colors: ButtonColors,
    theme: &theme::Theme,
) {
    widget.bg_fill = colors.fill;
    widget.weak_bg_fill = colors.fill;

    widget.bg_stroke =
        egui::Stroke::new(theme.metrics.border_width, colors.border);
    widget.fg_stroke =
        egui::Stroke::new(theme.metrics.foreground_stroke_width, colors.text);
}
