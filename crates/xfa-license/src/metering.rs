//! Usage metering and rate limiting.
//!
//! Tracks API calls, pages rendered, and forms processed per license.
//! Provides sliding-window rate limiting per minute.

use crate::claims::{LicenseClaims, Quotas};
use crate::error::{LicenseError, Result};
use std::collections::VecDeque;

/// Usage counters for a billing period.
#[derive(Debug, Clone, Default)]
pub struct UsageCounters {
    /// Total API calls in the current period.
    pub api_calls: u64,
    /// Total pages rendered in the current period.
    pub pages_rendered: u64,
    /// Total forms processed in the current period.
    pub forms_processed: u64,
}

impl UsageCounters {
    /// Reset all counters (e.g., at the start of a new billing period).
    pub fn reset(&mut self) {
        self.api_calls = 0;
        self.pages_rendered = 0;
        self.forms_processed = 0;
    }
}

/// Sliding window rate limiter.
///
/// Tracks request timestamps within a 60-second window.
#[derive(Debug)]
struct RateLimiter {
    /// Timestamps of recent requests (unix seconds).
    window: VecDeque<u64>,
    /// Maximum requests per 60-second window.
    max_per_minute: u32,
}

impl RateLimiter {
    fn new(max_per_minute: u32) -> Self {
        Self {
            window: VecDeque::new(),
            max_per_minute,
        }
    }

    /// Record a request at the given timestamp. Returns `Err` if rate limit exceeded.
    fn check(&mut self, now: u64) -> std::result::Result<(), u32> {
        let cutoff = now.saturating_sub(60);
        while self.window.front().is_some_and(|&t| t <= cutoff) {
            self.window.pop_front();
        }
        if self.window.len() >= self.max_per_minute as usize {
            return Err(self.max_per_minute);
        }
        self.window.push_back(now);
        Ok(())
    }

    /// Current number of requests in the window.
    fn current_count(&self) -> usize {
        self.window.len()
    }
}

/// Usage meter tied to a specific license.
///
/// Combines quota tracking and rate limiting for a single customer.
#[derive(Debug)]
pub struct UsageMeter {
    quotas: Quotas,
    counters: UsageCounters,
    rate_limiter: RateLimiter,
}

impl UsageMeter {
    /// Create a meter from license claims.
    pub fn from_claims(claims: &LicenseClaims) -> Self {
        Self {
            quotas: claims.quotas.clone(),
            counters: UsageCounters::default(),
            rate_limiter: RateLimiter::new(claims.rate_limit),
        }
    }

    /// Record an API call. Checks both rate limit and quota.
    ///
    /// `now` is the current unix timestamp in seconds.
    pub fn record_api_call(&mut self, now: u64) -> Result<()> {
        self.rate_limiter
            .check(now)
            .map_err(LicenseError::RateLimitExceeded)?;

        self.counters.api_calls += 1;
        if self.quotas.api_calls > 0 && self.counters.api_calls > self.quotas.api_calls {
            return Err(LicenseError::QuotaExceeded {
                resource: "api_calls".to_string(),
                used: self.counters.api_calls,
                limit: self.quotas.api_calls,
            });
        }
        Ok(())
    }

    /// Record pages rendered.
    pub fn record_pages(&mut self, count: u64) -> Result<()> {
        self.counters.pages_rendered += count;
        if self.quotas.pages_rendered > 0
            && self.counters.pages_rendered > self.quotas.pages_rendered
        {
            return Err(LicenseError::QuotaExceeded {
                resource: "pages_rendered".to_string(),
                used: self.counters.pages_rendered,
                limit: self.quotas.pages_rendered,
            });
        }
        Ok(())
    }

    /// Record a form processed.
    pub fn record_form(&mut self) -> Result<()> {
        self.counters.forms_processed += 1;
        if self.quotas.forms_processed > 0
            && self.counters.forms_processed > self.quotas.forms_processed
        {
            return Err(LicenseError::QuotaExceeded {
                resource: "forms_processed".to_string(),
                used: self.counters.forms_processed,
                limit: self.quotas.forms_processed,
            });
        }
        Ok(())
    }

    /// Get current usage counters.
    pub fn counters(&self) -> &UsageCounters {
        &self.counters
    }

    /// Get current rate limiter count (requests in the last minute).
    pub fn rate_current(&self) -> usize {
        self.rate_limiter.current_count()
    }

    /// Reset counters for a new billing period.
    pub fn reset_counters(&mut self) {
        self.counters.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claims::{LicenseClaims, Tier};

    fn trial_meter() -> UsageMeter {
        let claims = LicenseClaims::new("test", Tier::Trial, 1000, 2000);
        UsageMeter::from_claims(&claims)
    }

    #[test]
    fn api_calls_within_quota() {
        let mut meter = trial_meter(); // quota: 100, rate: 10/min
        // Space calls 61s apart so each starts a fresh rate window.
        for i in 0..100 {
            meter.record_api_call(1000 + i * 61).unwrap();
        }
        assert_eq!(meter.counters().api_calls, 100);
    }

    #[test]
    fn api_calls_exceed_quota() {
        let mut meter = trial_meter(); // quota: 100, rate: 10/min
        for i in 0..100 {
            meter.record_api_call(1000 + i * 61).unwrap();
        }
        let result = meter.record_api_call(1000 + 100 * 61);
        assert!(matches!(result, Err(LicenseError::QuotaExceeded { .. })));
    }

    #[test]
    fn rate_limit_enforced() {
        let mut meter = trial_meter(); // rate limit: 10/min
        let now = 5000;
        for i in 0..10 {
            meter.record_api_call(now + i).unwrap();
        }
        // 11th call within the same minute window → rate limited.
        let result = meter.record_api_call(now + 10);
        assert!(matches!(result, Err(LicenseError::RateLimitExceeded(10))));
    }

    #[test]
    fn rate_limit_window_slides() {
        let mut meter = trial_meter(); // 10/min
        // Fill window at the same timestamp.
        for _ in 0..10 {
            meter.record_api_call(1000).unwrap();
        }
        // After 61 seconds, all old entries expire (cutoff = 1061-60 = 1001 > 1000).
        meter.record_api_call(1061).unwrap();
        assert_eq!(meter.rate_current(), 1);
    }

    #[test]
    fn page_quota() {
        let mut meter = trial_meter(); // 500 pages
        meter.record_pages(400).unwrap();
        meter.record_pages(100).unwrap();
        let result = meter.record_pages(1);
        assert!(matches!(result, Err(LicenseError::QuotaExceeded { .. })));
    }

    #[test]
    fn unlimited_forms() {
        let mut meter = trial_meter(); // forms_processed quota = 0 (unlimited)
        for _ in 0..1000 {
            meter.record_form().unwrap();
        }
        assert_eq!(meter.counters().forms_processed, 1000);
    }

    #[test]
    fn reset_counters() {
        let mut meter = trial_meter();
        meter.record_api_call(1000).unwrap();
        meter.record_pages(10).unwrap();
        meter.record_form().unwrap();
        assert_eq!(meter.counters().api_calls, 1);

        meter.reset_counters();
        assert_eq!(meter.counters().api_calls, 0);
        assert_eq!(meter.counters().pages_rendered, 0);
        assert_eq!(meter.counters().forms_processed, 0);
    }

    #[test]
    fn enterprise_higher_limits() {
        let claims = LicenseClaims::new("ent", Tier::Enterprise, 1000, u64::MAX);
        let mut meter = UsageMeter::from_claims(&claims);
        // Enterprise: 1000 req/min. Fill at same timestamp.
        for _ in 0..1000 {
            meter.record_api_call(5000).unwrap();
        }
        // 1001st call at same timestamp → rate limited.
        let result = meter.record_api_call(5000);
        assert!(matches!(result, Err(LicenseError::RateLimitExceeded(1000))));
    }
}
