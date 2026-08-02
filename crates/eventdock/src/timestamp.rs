use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Wall-clock nanoseconds since the Unix epoch. Use Instant for durations.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(transparent)]
pub struct Timestamp {
    unix_nanos: i64,
}

impl Timestamp {
    #[must_use]
    pub const fn from_unix_nanos(unix_nanos: i64) -> Self {
        Self { unix_nanos }
    }

    #[must_use]
    pub const fn as_unix_nanos(self) -> i64 {
        self.unix_nanos
    }

    #[must_use]
    pub fn from_system_time(time: SystemTime) -> Option<Self> {
        let nanos = match time.duration_since(UNIX_EPOCH) {
            Ok(duration) => i128::try_from(duration.as_nanos()).ok()?,
            Err(error) => -i128::try_from(error.duration().as_nanos()).ok()?,
        };
        Some(Self {
            unix_nanos: i64::try_from(nanos).ok()?,
        })
    }

    #[must_use]
    pub fn now() -> Self {
        Self::from_system_time(SystemTime::now())
            .expect("system clock outside supported timestamp range")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn epoch() {
        assert_eq!(
            Timestamp::from_system_time(UNIX_EPOCH)
                .unwrap()
                .as_unix_nanos(),
            0
        );
    }

    #[test]
    fn before_and_after_epoch() {
        assert_eq!(
            Timestamp::from_system_time(UNIX_EPOCH + Duration::from_nanos(123))
                .unwrap()
                .as_unix_nanos(),
            123
        );
        assert_eq!(
            Timestamp::from_system_time(UNIX_EPOCH - Duration::from_nanos(123))
                .unwrap()
                .as_unix_nanos(),
            -123
        );
    }

    #[test]
    fn serde_integer() {
        let t = Timestamp::from_unix_nanos(123);
        assert_eq!(serde_json::to_string(&t).unwrap(), "123");
        assert_eq!(serde_json::from_str::<Timestamp>("123").unwrap(), t);
    }
}
