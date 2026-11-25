#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenerationalId {
    index: u32,
    generation: u32,
}

impl GenerationalId {
    pub(crate) fn new(
        index: u32,
        generation: u32,
    ) -> Self {
        Self { index, generation }
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }
}
