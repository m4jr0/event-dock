use eframe::egui;

const JAPANESE_FONT: &str = "noto_sans_jp";

pub(super) fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        JAPANESE_FONT.to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/NotoSansJP.ttf"
        ))
        .into(),
    );

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace]
    {
        fonts
            .families
            .entry(family)
            .or_default()
            .push(JAPANESE_FONT.to_owned());
    }

    ctx.set_fonts(fonts);
}
