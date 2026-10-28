use super::PrintError;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const PRINT_MIN_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(super) struct PrintRateLimiter {
    last_print: Mutex<Option<Instant>>,
}

impl PrintRateLimiter {
    pub(super) fn check(&self) -> Result<(), PrintError> {
        self.check_at(Instant::now())
    }

    fn check_at(&self, now: Instant) -> Result<(), PrintError> {
        let mut last = self
            .last_print
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if last.is_some_and(|previous| now.saturating_duration_since(previous) < PRINT_MIN_INTERVAL)
        {
            return Err(PrintError::RateLimited);
        }
        *last = Some(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{PrintRateLimiter, PRINT_MIN_INTERVAL};
    use std::time::{Duration, Instant};

    #[test]
    fn cooldown_rejects_rapid_calls_and_recovers_after_two_seconds() {
        let limiter = PrintRateLimiter::default();
        let start = Instant::now();
        assert!(limiter.check_at(start).is_ok());
        assert!(limiter
            .check_at(start + Duration::from_millis(500))
            .is_err());
        assert!(limiter.check_at(start + PRINT_MIN_INTERVAL).is_ok());
    }
}
