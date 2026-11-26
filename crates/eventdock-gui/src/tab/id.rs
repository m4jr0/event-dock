use eventdock::identity::GenerationalId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TabId(pub(super) GenerationalId);

#[cfg(debug_assertions)]
impl TabId {
    pub(crate) fn debug_index(self) -> impl std::fmt::Display {
        self.0.index()
    }
}
