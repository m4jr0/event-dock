use super::id::TabId;
use eventdock::identity::IdAllocator;

#[derive(Debug)]
pub(crate) struct Tab {
    id: TabId,
}

impl Tab {
    pub(crate) fn id(&self) -> TabId {
        self.id
    }
}

#[derive(Debug)]
pub(crate) struct TabManager {
    ids: IdAllocator,
    tabs: Vec<Tab>,
    active_tab: Option<TabId>,
}

impl TabManager {
    const DEFAULT_CAPACITY: usize = 16;

    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            ids: IdAllocator::with_capacity(capacity),
            tabs: Vec::with_capacity(capacity),
            active_tab: None,
        }
    }

    pub(crate) fn add_tab(&mut self) -> Option<TabId> {
        let id = TabId(self.ids.allocate()?);
        self.tabs.push(Tab { id });
        self.active_tab = Some(id);

        Some(id)
    }

    pub(crate) fn remove_tab(
        &mut self,
        id: TabId,
    ) -> bool {
        let Some(index) = self.position(id) else {
            return false;
        };

        if !self.ids.release(id.0) {
            return false;
        }

        self.tabs.remove(index);

        if self.active_tab == Some(id) {
            self.active_tab = self
                .tabs
                .get(index)
                .or_else(|| self.tabs.last())
                .map(Tab::id);
        }

        true
    }

    pub(crate) fn select_tab(
        &mut self,
        id: TabId,
    ) -> bool {
        if !self.ids.is_valid(id.0) {
            return false;
        }

        self.active_tab = Some(id);

        true
    }

    /// Destination is the final zero-based position.
    pub(crate) fn move_tab(
        &mut self,
        id: TabId,
        destination: usize,
    ) -> bool {
        if destination >= self.tabs.len() {
            return false;
        }

        let Some(source) = self.position(id) else {
            return false;
        };

        if source != destination {
            let tab = self.tabs.remove(source);
            self.tabs.insert(destination, tab);
        }

        true
    }

    pub(crate) fn active_tab_id(&self) -> Option<TabId> {
        self.active_tab
    }

    pub(crate) fn active_tab(&self) -> Option<&Tab> {
        self.active_tab
            .and_then(|id| self.get_tab(id))
    }

    pub(crate) fn get_tab(
        &self,
        id: TabId,
    ) -> Option<&Tab> {
        self.tabs
            .iter()
            .find(|tab| tab.id == id)
    }

    pub(crate) fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub(crate) fn len(&self) -> usize {
        self.tabs.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    fn position(
        &self,
        id: TabId,
    ) -> Option<usize> {
        self.tabs
            .iter()
            .position(|tab| tab.id == id)
    }
}

impl Default for TabManager {
    fn default() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(manager: &TabManager) -> Vec<TabId> {
        manager
            .tabs()
            .iter()
            .map(Tab::id)
            .collect()
    }

    #[test]
    fn new_manager_is_empty() {
        let manager = TabManager::new();

        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
        assert_eq!(manager.active_tab_id(), None);
        assert!(manager.active_tab().is_none());
    }

    #[test]
    fn adds_distinct_tabs_and_selects_latest() {
        let mut manager = TabManager::new();
        let a = manager.add_tab().unwrap();
        let b = manager.add_tab().unwrap();

        assert_ne!(a, b);
        assert_eq!(ids(&manager), vec![a, b]);
        assert_eq!(manager.len(), 2);
        assert_eq!(manager.active_tab_id(), Some(b));
        assert_eq!(manager.active_tab().map(Tab::id), Some(b));
        assert_eq!(manager.get_tab(a).map(Tab::id), Some(a));
    }

    #[test]
    fn selects_existing_tab() {
        let mut manager = TabManager::new();
        let a = manager.add_tab().unwrap();

        manager.add_tab().unwrap();

        assert!(manager.select_tab(a));
        assert_eq!(manager.active_tab_id(), Some(a));
    }

    #[test]
    fn removing_inactive_tab_preserves_selection() {
        let mut manager = TabManager::new();
        let a = manager.add_tab().unwrap();
        let b = manager.add_tab().unwrap();

        assert!(manager.remove_tab(a));
        assert_eq!(ids(&manager), vec![b]);
        assert_eq!(manager.active_tab_id(), Some(b));
    }

    #[test]
    fn removing_active_tab_selects_right_then_left() {
        let mut manager = TabManager::new();
        let a = manager.add_tab().unwrap();
        let b = manager.add_tab().unwrap();
        let c = manager.add_tab().unwrap();

        assert!(manager.select_tab(b));
        assert!(manager.remove_tab(b));
        assert_eq!(manager.active_tab_id(), Some(c));
        assert!(manager.remove_tab(c));
        assert_eq!(manager.active_tab_id(), Some(a));
        assert!(manager.remove_tab(a));
        assert!(manager.is_empty());
        assert_eq!(manager.active_tab_id(), None);
    }

    #[test]
    fn stale_id_cannot_affect_recycled_tab() {
        let mut manager = TabManager::new();
        let a = manager.add_tab().unwrap();

        assert!(manager.remove_tab(a));

        let b = manager.add_tab().unwrap();

        assert_ne!(a, b);
        assert_eq!(a.0.index(), b.0.index());
        assert!(manager.get_tab(a).is_none());
        assert!(!manager.select_tab(a));
        assert!(!manager.remove_tab(a));
        assert!(!manager.move_tab(a, 0));
        assert_eq!(manager.active_tab_id(), Some(b));
        assert_eq!(ids(&manager), vec![b]);
    }

    #[test]
    fn moves_tabs_without_changing_selection() {
        let mut manager = TabManager::new();
        let a = manager.add_tab().unwrap();
        let b = manager.add_tab().unwrap();
        let c = manager.add_tab().unwrap();

        assert!(manager.move_tab(c, 0));
        assert_eq!(ids(&manager), vec![c, a, b]);
        assert!(manager.move_tab(c, 2));
        assert_eq!(ids(&manager), vec![a, b, c]);
        assert!(manager.move_tab(c, 2));
        assert!(!manager.move_tab(a, 3));
        assert_eq!(ids(&manager), vec![a, b, c]);
        assert_eq!(manager.active_tab_id(), Some(c));
    }

    #[test]
    fn respects_initial_capacity() {
        let manager = TabManager::with_capacity(32);

        assert!(manager.tabs.capacity() >= 32);
        assert!(manager.is_empty());
    }
}
