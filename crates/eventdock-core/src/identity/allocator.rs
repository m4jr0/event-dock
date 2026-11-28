use super::id::GenerationalId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotState {
    Free,
    Allocated,
    Retired,
}

#[derive(Debug)]
struct Slot {
    generation: u32,
    state: SlotState,
}

impl Slot {
    fn new() -> Self {
        Self {
            generation: 0,
            state: SlotState::Free,
        }
    }

    fn acquire(&mut self) {
        debug_assert_eq!(self.state, SlotState::Free);
        self.state = SlotState::Allocated;
    }

    fn release(&mut self) {
        debug_assert_eq!(self.state, SlotState::Allocated);

        match self.generation.checked_add(1) {
            Some(generation) => {
                self.generation = generation;
                self.state = SlotState::Free;
            }

            None => {
                self.state = SlotState::Retired;
            }
        }
    }

    fn is_free(&self) -> bool {
        self.state == SlotState::Free
    }

    fn matches(
        &self,
        generation: u32,
    ) -> bool {
        self.state == SlotState::Allocated && self.generation == generation
    }
}

#[derive(Debug, Default)]
pub struct IdAllocator {
    slots: Vec<Slot>,
    free_indices: Vec<u32>,
    allocated_count: usize,
}

impl IdAllocator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            free_indices: Vec::with_capacity(capacity),
            allocated_count: 0,
        }
    }

    pub fn allocate(&mut self) -> Option<GenerationalId> {
        let index = if let Some(index) = self.free_indices.pop() {
            index
        } else {
            let index = u32::try_from(self.slots.len()).ok()?;

            self.slots.push(Slot::new());

            index
        };

        let slot = self.slot_mut(index)?;
        slot.acquire();

        let generation = slot.generation;
        self.allocated_count += 1;

        Some(GenerationalId::new(index, generation))
    }

    pub fn release(
        &mut self,
        id: GenerationalId,
    ) -> bool {
        let Some(slot) = self.slot_mut(id.index()) else {
            return false;
        };

        if !slot.matches(id.generation()) {
            return false;
        }

        slot.release();

        if slot.is_free() {
            self.free_indices.push(id.index());
        }

        self.allocated_count -= 1;

        true
    }

    pub fn is_valid(
        &self,
        id: GenerationalId,
    ) -> bool {
        self.slot(id.index())
            .is_some_and(|slot| slot.matches(id.generation()))
    }

    pub fn len(&self) -> usize {
        self.allocated_count
    }

    pub fn is_empty(&self) -> bool {
        self.allocated_count == 0
    }

    fn slot_mut(
        &mut self,
        index: u32,
    ) -> Option<&mut Slot> {
        let index = usize::try_from(index).ok()?;

        self.slots.get_mut(index)
    }

    fn slot(
        &self,
        index: u32,
    ) -> Option<&Slot> {
        let index = usize::try_from(index).ok()?;

        self.slots.get(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_allocator_is_empty() {
        let allocator = IdAllocator::new();

        assert!(allocator.is_empty());
        assert_eq!(allocator.len(), 0);
    }

    #[test]
    fn allocates_distinct_ids() {
        let mut allocator = IdAllocator::new();

        let first = allocator.allocate().unwrap();
        let second = allocator.allocate().unwrap();

        assert_ne!(first, second);
        assert!(allocator.is_valid(first));
        assert!(allocator.is_valid(second));
        assert_eq!(allocator.len(), 2);
    }

    #[test]
    fn releases_id() {
        let mut allocator = IdAllocator::new();
        let id = allocator.allocate().unwrap();

        assert!(allocator.release(id));
        assert!(!allocator.is_valid(id));
        assert!(allocator.is_empty());
    }

    #[test]
    fn reuses_index_with_new_generation() {
        let mut allocator = IdAllocator::new();

        let first = allocator.allocate().unwrap();
        assert!(allocator.release(first));

        let second = allocator.allocate().unwrap();

        assert_eq!(first.index(), second.index());
        assert_ne!(first.generation(), second.generation());
        assert_ne!(first, second);

        assert!(!allocator.is_valid(first));
        assert!(allocator.is_valid(second));
    }

    #[test]
    fn rejects_double_release() {
        let mut allocator = IdAllocator::new();
        let id = allocator.allocate().unwrap();

        assert!(allocator.release(id));
        assert!(!allocator.release(id));
        assert!(allocator.is_empty());
    }

    #[test]
    fn stale_id_cannot_release_reused_slot() {
        let mut allocator = IdAllocator::new();

        let first = allocator.allocate().unwrap();
        assert!(allocator.release(first));

        let second = allocator.allocate().unwrap();

        assert!(!allocator.release(first));
        assert!(allocator.is_valid(second));
        assert_eq!(allocator.len(), 1);
    }

    #[test]
    fn rejects_unknown_index() {
        let mut allocator = IdAllocator::new();
        let id = GenerationalId::new(42, 0);

        assert!(!allocator.is_valid(id));
        assert!(!allocator.release(id));
    }

    #[test]
    fn releasing_preserves_other_ids() {
        let mut allocator = IdAllocator::new();

        let first = allocator.allocate().unwrap();
        let second = allocator.allocate().unwrap();

        assert!(allocator.release(first));
        assert!(allocator.is_valid(second));
        assert_eq!(allocator.len(), 1);
    }

    #[test]
    fn retires_slot_on_generation_exhaustion() {
        let mut allocator = IdAllocator::new();

        allocator.slots.push(Slot {
            generation: u32::MAX,
            state: SlotState::Allocated,
        });

        allocator.allocated_count = 1;

        let id = GenerationalId::new(0, u32::MAX);

        assert!(allocator.release(id));
        assert_eq!(allocator.slots[0].state, SlotState::Retired);
        assert!(allocator.free_indices.is_empty());
        assert!(allocator.is_empty());

        let next = allocator.allocate().unwrap();

        assert_ne!(next.index(), id.index());
        assert!(!allocator.is_valid(id));
    }
}
