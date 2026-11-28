//! Carry admitted ingestion work and its reservation together.
use crate::ByteReservation;
use eventdock_core::IncomingEvent;

#[derive(Debug)]
pub(crate) struct AdmittedEvent {
    pub event: IncomingEvent,
    _reservation: ByteReservation,
}

impl AdmittedEvent {
    pub(crate) fn new(
        event: IncomingEvent,
        reservation: ByteReservation,
    ) -> Self {
        Self {
            event,
            _reservation: reservation,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(event: IncomingEvent) -> Self {
        let budget = crate::ByteBudget::new(1).unwrap();
        Self::new(event, budget.try_reserve(1).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eventdock_core::LogLevel;

    #[test]
    fn queued_event_holds_budget_until_destroyed() {
        let budget = crate::ByteBudget::new(10).unwrap();
        let (sender, receiver) = flume::bounded(2);
        let event = IncomingEvent::new("test", LogLevel::Info, "message");
        sender
            .try_send(AdmittedEvent::new(
                event,
                budget.try_reserve(10).unwrap(),
            ))
            .unwrap();
        assert!(budget.try_reserve(1).is_none());
        let event = receiver.try_recv().unwrap();
        assert!(budget.try_reserve(1).is_none());
        drop(event);
        assert_eq!(budget.available_bytes(), 10);
    }
}
