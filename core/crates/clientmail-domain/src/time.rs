/// A point in time, as whole seconds since the Unix epoch, in UTC.
///
/// The core only orders and compares times. Formatting for people happens in the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnixTime(i64);

impl UnixTime {
    /// Creates a time from seconds since the Unix epoch.
    ///
    /// # Arguments
    ///
    /// * `seconds` - Seconds since 1970-01-01T00:00:00Z. Negative values are earlier.
    #[must_use]
    pub const fn from_seconds(seconds: i64) -> Self {
        Self(seconds)
    }

    /// Returns the seconds since the Unix epoch.
    #[must_use]
    pub const fn seconds(self) -> i64 {
        self.0
    }

    /// Returns this time moved by `seconds`, clamped to the representable range.
    ///
    /// # Arguments
    ///
    /// * `seconds` - How far to move. Negative values move back in time.
    ///
    /// # Examples
    ///
    /// ```
    /// use clientmail_domain::UnixTime;
    ///
    /// let now = UnixTime::from_seconds(10_000_000);
    /// let thirty_days_ago = now.saturating_add_seconds(-30 * 86_400);
    /// assert_eq!(thirty_days_ago.seconds(), 7_408_000);
    /// ```
    #[must_use]
    pub const fn saturating_add_seconds(self, seconds: i64) -> Self {
        Self(self.0.saturating_add(seconds))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECONDS_PER_DAY: i64 = 86_400;

    #[test]
    fn times_order_by_seconds() {
        let earlier = UnixTime::from_seconds(-5);
        let later = UnixTime::from_seconds(10);

        assert!(earlier < later, "negative seconds come first");
        assert_eq!(later.seconds(), 10, "seconds round-trip");
    }

    #[test]
    fn older_than_window_uses_saturating_subtraction() {
        let now = UnixTime::from_seconds(100 * SECONDS_PER_DAY);
        let window_start = now.saturating_add_seconds(-30 * SECONDS_PER_DAY);

        assert!(
            UnixTime::from_seconds(69 * SECONDS_PER_DAY) < window_start,
            "31 days old is outside a 30-day window"
        );
        assert!(
            UnixTime::from_seconds(71 * SECONDS_PER_DAY) > window_start,
            "29 days old is inside a 30-day window"
        );
    }

    #[test]
    fn saturating_add_clamps_at_the_limits() {
        let max = UnixTime::from_seconds(i64::MAX);
        let min = UnixTime::from_seconds(i64::MIN);

        assert_eq!(max.saturating_add_seconds(1), max, "clamps at the top");
        assert_eq!(min.saturating_add_seconds(-1), min, "clamps at the bottom");
    }
}
