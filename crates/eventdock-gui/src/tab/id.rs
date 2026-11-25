use eventdock::identity::GenerationalId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TabId(pub(super) GenerationalId);
