use eframe::egui::Color32;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SurfaceColors {
    pub(crate) background: Color32,
    pub(crate) panel: Color32,
    pub(crate) raised: Color32,
    pub(crate) border: Color32,
    pub(crate) transparent: Color32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TextColors {
    pub(crate) primary: Color32,
    pub(crate) secondary: Color32,
    pub(crate) disabled: Color32,
    pub(crate) on_accent: Color32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct InteractionColors {
    pub(crate) accent: Color32,
    pub(crate) accent_hovered: Color32,
    pub(crate) accent_pressed: Color32,
    pub(crate) hovered: Color32,
    pub(crate) pressed: Color32,
    pub(crate) selected: Color32,
    pub(crate) focus: Color32,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Reserved for upcoming log and status widgets.
pub(crate) struct StatusColors {
    pub(crate) success: Color32,
    pub(crate) warning: Color32,
    pub(crate) error: Color32,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)] // Reserved for upcoming log and status widgets.
pub(crate) struct SeverityColors {
    pub(crate) trace: Color32,
    pub(crate) debug: Color32,
    pub(crate) info: Color32,
    pub(crate) warning: Color32,
    pub(crate) error: Color32,
    pub(crate) critical: Color32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Palette {
    pub(crate) surface: SurfaceColors,
    pub(crate) text: TextColors,
    pub(crate) interaction: InteractionColors,
    pub(crate) status: StatusColors,
    #[allow(dead_code)]
    pub(crate) severity: SeverityColors,
}
