//! Shared byte reservations for bounded resource admission.

use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Share a fixed byte budget across independent owners.
#[derive(Debug, Clone)]
pub struct ByteBudget {
    semaphore: Arc<Semaphore>,
    capacity: usize,
}

/// Return reserved bytes automatically when the reservation is dropped.
#[derive(Debug)]
pub struct ByteReservation {
    _permit: OwnedSemaphorePermit,
    bytes: usize,
}

impl ByteBudget {
    /// Limit capacity to the range supported by multi-permit acquisition.
    pub const MAX_CAPACITY: usize =
        if Semaphore::MAX_PERMITS < u32::MAX as usize {
            Semaphore::MAX_PERMITS
        } else {
            u32::MAX as usize
        };

    pub fn new(capacity: usize) -> Option<Self> {
        if capacity == 0 || capacity > Self::MAX_CAPACITY {
            return None;
        }
        Some(Self {
            semaphore: Arc::new(Semaphore::new(capacity)),
            capacity,
        })
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Include permits assigned to waiting acquisitions in unavailable bytes.
    pub fn available_bytes(&self) -> usize {
        self.semaphore.available_permits()
    }

    fn permit_count(
        &self,
        bytes: usize,
    ) -> Option<u32> {
        (bytes > 0 && bytes <= self.capacity).then_some(bytes as u32)
    }

    /// Reserve immediately or return None without waiting.
    pub fn try_reserve(
        &self,
        bytes: usize,
    ) -> Option<ByteReservation> {
        let count = self.permit_count(bytes)?;
        let permit = Arc::clone(&self.semaphore)
            .try_acquire_many_owned(count)
            .ok()?;
        Some(ByteReservation {
            _permit: permit,
            bytes,
        })
    }

    /// Wait asynchronously for space; reject impossible requests immediately.
    pub async fn reserve(
        &self,
        bytes: usize,
    ) -> Option<ByteReservation> {
        let count = self.permit_count(bytes)?;
        let permit = Arc::clone(&self.semaphore)
            .acquire_many_owned(count)
            .await
            .ok()?;
        Some(ByteReservation {
            _permit: permit,
            bytes,
        })
    }
}

impl ByteReservation {
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };

    #[test]
    fn rejects_invalid_capacities_and_requests() {
        assert!(ByteBudget::new(0).is_none());
        assert!(ByteBudget::new(ByteBudget::MAX_CAPACITY + 1).is_none());
        let budget = ByteBudget::new(10).unwrap();
        assert!(budget.try_reserve(0).is_none());
        assert!(budget.try_reserve(11).is_none());
        assert_eq!(budget.available_bytes(), 10);
    }

    #[test]
    fn cloned_budgets_share_capacity_and_release_on_drop() {
        let budget = ByteBudget::new(10).unwrap();
        let other = budget.clone();
        let first = budget.try_reserve(7).unwrap();
        assert_eq!(first.bytes(), 7);
        assert_eq!(other.capacity(), 10);
        assert!(other.try_reserve(4).is_none());
        let second = other.try_reserve(3).unwrap();
        assert_eq!(budget.available_bytes(), 0);
        drop(first);
        assert_eq!(other.available_bytes(), 7);
        drop(second);
        assert_eq!(budget.available_bytes(), 10);
    }

    #[test]
    fn cancelling_wait_returns_assigned_permits() {
        let budget = ByteBudget::new(10).unwrap();
        let held = budget.try_reserve(7).unwrap();
        let mut waiting = Box::pin(budget.reserve(8));
        let mut context = Context::from_waker(Waker::noop());
        assert!(
            waiting
                .as_mut()
                .poll(&mut context)
                .is_pending()
        );
        drop(waiting);
        assert_eq!(budget.available_bytes(), 3);
        drop(held);
        assert_eq!(budget.available_bytes(), 10);
    }

    #[test]
    fn waiting_reservation_completes_after_release() {
        let budget = ByteBudget::new(10).unwrap();
        let held = budget.try_reserve(10).unwrap();
        let mut waiting = Box::pin(budget.reserve(8));
        let mut context = Context::from_waker(Waker::noop());
        assert!(
            waiting
                .as_mut()
                .poll(&mut context)
                .is_pending()
        );
        drop(held);
        let Poll::Ready(Some(reservation)) =
            waiting.as_mut().poll(&mut context)
        else {
            panic!("reservation should be ready");
        };
        assert_eq!(reservation.bytes(), 8);
        assert_eq!(budget.available_bytes(), 2);
        drop(reservation);
        assert_eq!(budget.available_bytes(), 10);
    }
}
