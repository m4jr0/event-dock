use rust_i18n::t;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct Locale(String);

impl Default for Locale {
    fn default() -> Self {
        Self("en".to_owned())
    }
}

impl Locale {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn apply(&self) {
        rust_i18n::set_locale(self.as_str());
    }
}

pub(crate) struct LocaleInfo {
    pub(crate) locale: Locale,
    pub(crate) name: String,
}

pub(crate) struct LocaleCatalog {
    entries: Vec<LocaleInfo>,
}

impl LocaleCatalog {
    pub(crate) fn new() -> Self {
        let mut entries: Vec<_> = rust_i18n::available_locales!()
            .iter()
            .map(|code| {
                let code = code.to_string();
                let name =
                    t!("language.name", locale = code.as_str()).to_string();
                LocaleInfo {
                    locale: Locale(code),
                    name,
                }
            })
            .collect();

        entries.sort_by(|a, b| a.locale.as_str().cmp(b.locale.as_str()));
        Self { entries }
    }

    pub(crate) fn entries(&self) -> &[LocaleInfo] {
        &self.entries
    }

    pub(crate) fn contains(
        &self,
        locale: &Locale,
    ) -> bool {
        self.entries
            .iter()
            .any(|entry| &entry.locale == locale)
    }
}
