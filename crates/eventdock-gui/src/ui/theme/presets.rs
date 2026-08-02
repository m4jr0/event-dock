use super::palette::*;
use super::{Metrics, Theme, Typography};
use eframe::egui::Color32;

const METRICS: Metrics = Metrics {
    spacing_small: 4.0,
    spacing_medium: 8.0,
    spacing_large: 16.0,
    corner_radius: 4,
    border_width: 1.0,
    foreground_stroke_width: 1.0,
    control_height: 28.0,
    row_height: 24.0,
};

const TYPOGRAPHY: Typography = Typography {
    small: 11.0,
    body: 13.0,
    heading: 18.0,
    monospace: 12.0,
};

const fn rgb(value: u32) -> Color32 {
    Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

pub(super) static DARK: Theme = Theme {
    palette: Palette {
        surface: SurfaceColors {
            background: rgb(0x15171B),
            panel: rgb(0x1C1F24),
            raised: rgb(0x252930),
            border: rgb(0x333840),
            transparent: Color32::TRANSPARENT,
        },

        text: TextColors {
            primary: rgb(0xDCE0E6),
            secondary: rgb(0x89929F),
            disabled: rgb(0x5D6570),
            on_accent: rgb(0x15171B),
        },

        interaction: InteractionColors {
            accent: rgb(0x82AAFF),
            accent_hovered: rgb(0x9AB9FF),
            accent_pressed: rgb(0x6D94E5),
            hovered: rgb(0x303640),
            pressed: rgb(0x394451),
            selected: rgb(0x293A52),
            focus: rgb(0x82AAFF),
        },

        status: StatusColors {
            success: rgb(0x78C6A3),
            warning: rgb(0xE4B66A),
            error: rgb(0xEE8383),
        },

        severity: SeverityColors {
            trace: rgb(0x8B94A3),
            debug: rgb(0xA99AE8),
            info: rgb(0x82AAFF),
            warning: rgb(0xE4B66A),
            error: rgb(0xEE8383),
            critical: rgb(0xF08CB8),
        },
    },

    metrics: METRICS,
    typography: TYPOGRAPHY,
};

pub(super) static LIGHT: Theme = Theme {
    palette: Palette {
        surface: SurfaceColors {
            background: rgb(0xF7F8FA),
            panel: rgb(0xFFFFFF),
            raised: rgb(0xEBEEF2),
            border: rgb(0xD6DAE0),
            transparent: Color32::TRANSPARENT,
        },

        text: TextColors {
            primary: rgb(0x252B34),
            secondary: rgb(0x687382),
            disabled: rgb(0x89929F),
            on_accent: rgb(0xFFFFFF),
        },

        interaction: InteractionColors {
            accent: rgb(0x315EB5),
            accent_hovered: rgb(0x284F9B),
            accent_pressed: rgb(0x203F80),
            hovered: rgb(0xE2E7EE),
            pressed: rgb(0xD2DCEB),
            selected: rgb(0xDCE8FC),
            focus: rgb(0x315EB5),
        },

        status: StatusColors {
            success: rgb(0x26734F),
            warning: rgb(0x936000),
            error: rgb(0xB83B3B),
        },

        severity: SeverityColors {
            trace: rgb(0x737D8A),
            debug: rgb(0x7356B4),
            info: rgb(0x315EB5),
            warning: rgb(0x936000),
            error: rgb(0xB83B3B),
            critical: rgb(0xA52A67),
        },
    },

    metrics: METRICS,
    typography: TYPOGRAPHY,
};
