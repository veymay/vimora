//! Retry delays that grow, never exceed a ceiling, and give up after a bounded number of tries
//! (no rapid reconnect loop against a receiver that is gone).

use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Backoff {
    base: Duration,
    max: Duration,
    max_attempts: u32,
    attempts: u32,
}

impl Backoff {
    pub fn new(base: Duration, max: Duration, max_attempts: u32) -> Self {
        Self {
            base,
            max,
            max_attempts,
            attempts: 0,
        }
    }

    /// Delay before the next try, or `None` when the tries are used up.
    pub fn next_delay(&mut self) -> Option<Duration> {
        if self.attempts >= self.max_attempts {
            return None;
        }
        let factor = 1u32.checked_shl(self.attempts).unwrap_or(u32::MAX);
        self.attempts += 1;
        Some(self.base.saturating_mul(factor).min(self.max))
    }

    /// A stable stretch: start from the beginning again.
    pub fn reset(&mut self) {
        self.attempts = 0;
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_up_to_the_ceiling_then_stops() {
        let mut b = Backoff::new(Duration::from_secs(1), Duration::from_secs(8), 6);
        let got: Vec<u64> = std::iter::from_fn(|| b.next_delay())
            .map(|d| d.as_secs())
            .collect();
        assert_eq!(got, [1, 2, 4, 8, 8, 8]);
        assert_eq!(b.next_delay(), None);
        b.reset();
        assert_eq!(b.next_delay(), Some(Duration::from_secs(1)));
    }

    #[test]
    fn huge_attempt_counts_do_not_overflow() {
        let mut b = Backoff::new(Duration::from_secs(1), Duration::from_secs(30), 100);
        for _ in 0..100 {
            assert!(b.next_delay().unwrap() <= Duration::from_secs(30));
        }
    }
}
