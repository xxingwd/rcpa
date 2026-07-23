use rand::Rng;
use std::time::Duration;

use crate::config::RetryConfig;

/// Retry policy with exponential backoff and jitter
pub struct RetryPolicy {
    max_attempts: u32,
    initial_backoff: Duration,
    max_backoff: Duration,
    retryable_statuses: Vec<u16>,
}

impl RetryPolicy {
    pub fn from_config(config: &RetryConfig) -> Self {
        Self {
            max_attempts: config.max_attempts,
            initial_backoff: Duration::from_millis(config.initial_backoff_ms),
            max_backoff: Duration::from_millis(config.max_backoff_ms),
            retryable_statuses: config.retryable_statuses.clone(),
        }
    }

    /// Check if a status code should be retried
    pub fn should_retry(&self, status: u16) -> bool {
        self.retryable_statuses.contains(&status)
    }

    /// Calculate backoff duration for attempt N (0-indexed)
    pub fn backoff_for(&self, attempt: u32) -> Duration {
        let backoff = self.initial_backoff * 2u32.pow(attempt);
        let capped = std::cmp::min(backoff, self.max_backoff);

        // Add jitter: ±25%
        let jitter_range = capped / 4;
        let mut rng = rand::thread_rng();
        let jitter = rng.gen_range(Duration::ZERO..jitter_range);

        capped + jitter
    }

    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_retry() {
        let policy = RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            retryable_statuses: vec![429, 502, 503],
        };
        assert!(policy.should_retry(429));
        assert!(policy.should_retry(502));
        assert!(!policy.should_retry(200));
        assert!(!policy.should_retry(400));
    }

    #[test]
    fn test_backoff_exponential() {
        let policy = RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            retryable_statuses: vec![],
        };
        let b0 = policy.backoff_for(0);
        let b1 = policy.backoff_for(1);
        let b2 = policy.backoff_for(2);
        assert!(b0 <= Duration::from_millis(150)); // 100ms + jitter
        assert!(b1 >= b0);
        assert!(b2 >= b1);
    }

    #[test]
    fn test_backoff_capped() {
        let policy = RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(200),
            retryable_statuses: vec![],
        };
        let b = policy.backoff_for(10); // Large attempt number
        assert!(b <= Duration::from_millis(250)); // 200 + 25%
    }
}
