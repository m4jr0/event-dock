use eframe::egui;
use rust_i18n::t;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SearchMode {
    #[allow(dead_code)] // Available to future dropdown callers.
    Never,
    #[allow(dead_code)] // Available to future dropdown callers.
    Always,
    Auto(usize),
}

impl Default for SearchMode {
    fn default() -> Self {
        Self::Auto(8)
    }
}

impl SearchMode {
    fn is_enabled(
        self,
        entry_count: usize,
    ) -> bool {
        match self {
            Self::Never => false,
            Self::Always => true,
            Self::Auto(threshold) => entry_count > threshold,
        }
    }
}

#[derive(Default)]
pub(crate) struct DropdownState {
    pub(crate) search_mode: SearchMode,
    query: String,
    was_open: bool,
}

struct Options<'a, T, S, L, K> {
    entries: &'a [T],
    selected: S,
    label: L,
    search_key: K,
}

pub(crate) fn show<T>(
    ui: &mut egui::Ui,
    id: &str,
    state: &mut DropdownState,
    entries: &[T],
    selected: impl Fn(&T) -> bool,
    label: impl Fn(&T) -> &str,
    search_key: impl Fn(&T) -> &str,
) -> Option<usize> {
    let options = Options {
        entries,
        selected,
        label,
        search_key,
    };

    let response = egui::ComboBox::from_id_salt(id)
        .selected_text(options.selected_text())
        .width(ui.available_width())
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| options.show_popup(ui, state));

    state.was_open = response.inner.is_some();
    response.inner.flatten()
}

impl<T, S, L, K> Options<'_, T, S, L, K>
where
    S: Fn(&T) -> bool,
    L: Fn(&T) -> &str,
    K: Fn(&T) -> &str,
{
    fn selected_text(&self) -> &str {
        self.entries
            .iter()
            .find(|entry| (self.selected)(entry))
            .map(&self.label)
            .unwrap_or_default()
    }

    fn show_popup(
        &self,
        ui: &mut egui::Ui,
        state: &mut DropdownState,
    ) -> Option<usize> {
        if !state.was_open {
            state.query.clear();
        }

        let query = if state
            .search_mode
            .is_enabled(self.entries.len())
        {
            show_search(ui, state);
            state.query.trim().to_lowercase()
        } else {
            String::new()
        };

        self.show_entries(ui, &query)
    }

    fn show_entries(
        &self,
        ui: &mut egui::Ui,
        query: &str,
    ) -> Option<usize> {
        let mut choice = None;
        let mut found = false;

        for (index, entry) in self.entries.iter().enumerate() {
            let name = (self.label)(entry);

            if !matches_query(name, || (self.search_key)(entry), query) {
                continue;
            }

            found = true;
            let selected = (self.selected)(entry);

            if ui
                .selectable_label(selected, name)
                .clicked()
            {
                if !selected {
                    choice = Some(index);
                }
                ui.close();
            }
        }

        if !found {
            ui.label(t!("settings.no_matching_options"));
        }

        choice
    }
}

fn show_search(
    ui: &mut egui::Ui,
    state: &mut DropdownState,
) {
    let response = ui.add(
        egui::TextEdit::singleline(&mut state.query)
            .hint_text(t!("settings.search_options")),
    );

    if !state.was_open {
        response.request_focus();
    }

    ui.separator();
}

fn matches_query<'a>(
    name: &str,
    search_key: impl FnOnce() -> &'a str,
    query: &str,
) -> bool {
    query.is_empty()
        || name.to_lowercase().contains(query)
        || search_key()
            .to_lowercase()
            .contains(query)
}

#[cfg(test)]
mod tests {
    use super::SearchMode;

    #[test]
    fn default_search_mode() {
        let mode = SearchMode::default();
        assert_eq!(mode, SearchMode::Auto(8));
        assert!(!mode.is_enabled(8));
        assert!(mode.is_enabled(9));
    }

    #[test]
    fn explicit_search_modes() {
        assert!(!SearchMode::Never.is_enabled(100));
        assert!(SearchMode::Always.is_enabled(0));
        assert!(!SearchMode::Auto(3).is_enabled(3));
        assert!(SearchMode::Auto(3).is_enabled(4));
    }
}
