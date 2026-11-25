mod action;
mod app;
mod config;
mod i18n;
mod input;
mod settings;
mod tab;
mod ui;

rust_i18n::i18n!("locales", fallback = "en");

fn main() -> eframe::Result {
    app::run(config::AppConfig::default())
}
