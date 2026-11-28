//! Configured resource allowances for admission planning.

/// Aggregate independent resource dimensions without treating them as heap bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResourceLimits {
    pub retained_payload_bytes: usize,
    pub retained_events: usize,
    pub ingestion_frame_bytes: usize,
    pub queued_events: usize,
    pub queued_queries: usize,
    pub producer_connections: usize,
}

impl ResourceLimits {
    /// Combine allowances, rejecting arithmetic overflow.
    pub fn checked_add(
        self,
        other: Self,
    ) -> Option<Self> {
        Some(Self {
            retained_payload_bytes: self
                .retained_payload_bytes
                .checked_add(other.retained_payload_bytes)?,
            retained_events: self
                .retained_events
                .checked_add(other.retained_events)?,
            ingestion_frame_bytes: self
                .ingestion_frame_bytes
                .checked_add(other.ingestion_frame_bytes)?,
            queued_events: self
                .queued_events
                .checked_add(other.queued_events)?,
            queued_queries: self
                .queued_queries
                .checked_add(other.queued_queries)?,
            producer_connections: self
                .producer_connections
                .checked_add(other.producer_connections)?,
        })
    }

    /// Check every independent allowance against its corresponding ceiling.
    pub fn fits_within(
        self,
        ceiling: Self,
    ) -> bool {
        self.retained_payload_bytes <= ceiling.retained_payload_bytes
            && self.retained_events <= ceiling.retained_events
            && self.ingestion_frame_bytes <= ceiling.ingestion_frame_bytes
            && self.queued_events <= ceiling.queued_events
            && self.queued_queries <= ceiling.queued_queries
            && self.producer_connections <= ceiling.producer_connections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_and_check_allowances() {
        let single = crate::BackendConfig::default().resource_limits();
        let combined = single.checked_add(single).unwrap();
        assert_eq!(
            combined.retained_payload_bytes,
            2 * single.retained_payload_bytes
        );
        assert!(single.fits_within(combined));
        assert!(!combined.fits_within(single));
        assert!(combined.fits_within(combined));
        let ceiling = ResourceLimits {
            queued_queries: combined.queued_queries - 1,
            ..combined
        };
        assert!(!combined.fits_within(ceiling));
    }

    #[test]
    fn reject_overflow() {
        let limits = ResourceLimits {
            retained_events: usize::MAX,
            ..ResourceLimits::default()
        };
        assert!(limits.checked_add(limits).is_none());
    }
}
