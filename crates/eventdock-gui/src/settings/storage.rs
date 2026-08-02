use super::{
    Settings,
    document::{decode, encode},
};

const KEY: &str = "eventdock.settings";

pub(crate) fn load(
    storage: Option<&dyn eframe::Storage>
) -> Result<Option<Settings>, String> {
    storage
        .and_then(|storage| storage.get_string(KEY))
        .map(|text| decode(&text))
        .transpose()
}

pub(crate) fn save(
    storage: &mut dyn eframe::Storage,
    settings: &Settings,
) -> Result<(), String> {
    storage.set_string(KEY, encode(settings)?);

    Ok(())
}
