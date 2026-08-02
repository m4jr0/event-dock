use super::{
    Settings,
    document::{decode, encode},
};
use std::{fs, path::Path};

pub(crate) fn import(path: &Path) -> Result<Settings, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    decode(&text)
}

pub(crate) fn export(
    path: &Path,
    settings: &Settings,
) -> Result<(), String> {
    fs::write(path, encode(settings)?).map_err(|error| error.to_string())
}

pub(super) fn pick_import() -> Result<Option<Settings>, String> {
    rfd::FileDialog::new()
        .pick_file()
        .map(|path| import(&path))
        .transpose()
}

pub(super) fn pick_export(settings: &Settings) -> Result<(), String> {
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("JSON", &["json"])
        .set_file_name("eventdock-settings.json")
        .save_file()
    {
        export(&path, settings)?;
    }

    Ok(())
}
