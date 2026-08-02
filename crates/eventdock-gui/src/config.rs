use crate::settings::Settings;

pub(crate) const APP_NAME: &str = "EventDock";

#[derive(Debug, Clone)]
pub(crate) struct AppConfig {
    pub(crate) name: String,
    pub(crate) size: [f32; 2],
    pub(crate) settings: Settings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: APP_NAME.to_owned(),
            size: [1280.0, 720.0],
            settings: Settings::default(),
        }
    }
}
