use crate::settings::Settings;
use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Document {
    version: u32,
    settings: Settings,
}

pub(crate) fn encode(settings: &Settings) -> Result<String, String> {
    serde_json::to_string_pretty(&Document {
        version: VERSION,
        settings: settings.clone(),
    })
    .map_err(|error| error.to_string())
}

pub(crate) fn decode(text: &str) -> Result<Settings, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| error.to_string())?;

    if value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        != Some(u64::from(VERSION))
    {
        return Err("Unsupported settings version".to_owned());
    }

    let document: Document =
        serde_json::from_value(value).map_err(|error| error.to_string())?;

    Ok(document.settings)
}

// Keep the on-disk representation independent of egui's serde format.
pub(crate) mod theme_preference {
    use eframe::egui::ThemePreference;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum Preference {
        System,
        Light,
        Dark,
    }

    pub(crate) fn serialize<S: Serializer>(
        value: &ThemePreference,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            ThemePreference::System => Preference::System,
            ThemePreference::Light => Preference::Light,
            ThemePreference::Dark => Preference::Dark,
        }
        .serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D
    ) -> Result<ThemePreference, D::Error> {
        Ok(match Preference::deserialize(deserializer)? {
            Preference::System => ThemePreference::System,
            Preference::Light => ThemePreference::Light,
            Preference::Dark => ThemePreference::Dark,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::ThemePreference;

    #[test]
    fn round_trip() {
        for theme in [
            ThemePreference::System,
            ThemePreference::Light,
            ThemePreference::Dark,
        ] {
            let settings = Settings {
                theme,
                ..Settings::default()
            };

            assert_eq!(decode(&encode(&settings).unwrap()).unwrap(), settings);
        }
    }

    #[test]
    fn missing_fields_use_defaults() {
        assert_eq!(
            decode(r#"{"version":1,"settings":{}}"#).unwrap(),
            Settings::default()
        );
    }

    #[test]
    fn rejects_invalid_documents() {
        for text in [
            "invalid",
            r#"{"version":2,"settings":{}}"#,
            r#"{"version":1,"settings":{"theme":"unknown"}}"#,
        ] {
            assert!(decode(text).is_err());
        }
    }
}
