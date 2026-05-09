use std::collections::BTreeSet;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    Disabled,
    Standard,
    Refresh,
    OnlyIfCached,
}

#[derive(Debug, Clone)]
pub struct CachePolicy {
    pub mode: CacheMode,
    pub default_ttl: Duration,
    pub stale_if_error: Duration,
    pub vary_headers: Vec<String>,
    pub max_entries: usize,
    pub cacheable_statuses: BTreeSet<u16>,
}

impl CachePolicy {
    pub fn enabled(&self) -> bool {
        !matches!(self.mode, CacheMode::Disabled)
    }

    pub fn should_store_status(&self, status: u16) -> bool {
        self.cacheable_statuses.contains(&status)
    }
}

impl Default for CachePolicy {
    fn default() -> Self {
        let mut statuses = BTreeSet::new();
        statuses.insert(200);
        statuses.insert(203);
        statuses.insert(204);
        statuses.insert(206);
        Self {
            mode: CacheMode::Standard,
            default_ttl: Duration::from_secs(30),
            stale_if_error: Duration::from_secs(10),
            vary_headers: vec!["accept".into(), "accept-language".into()],
            max_entries: 1_024,
            cacheable_statuses: statuses,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsSelectionStrategy {
    RoundRobin,
    RegionFirst,
    PrimaryOnly,
}

#[derive(Debug, Clone)]
pub struct DnsPolicy {
    pub prefer_ipv6: bool,
    pub ttl_floor: Duration,
    pub strategy: DnsSelectionStrategy,
    pub retry_on_resolve_error: bool,
}

impl Default for DnsPolicy {
    fn default() -> Self {
        Self {
            prefer_ipv6: false,
            ttl_floor: Duration::from_secs(5),
            strategy: DnsSelectionStrategy::RoundRobin,
            retry_on_resolve_error: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CdnStrategy {
    Disabled,
    Weighted,
    RegionAffinity,
}

#[derive(Debug, Clone)]
pub struct CdnPolicy {
    pub strategy: CdnStrategy,
    pub fallback_to_origin: bool,
    pub sticky_by_region: bool,
}

impl Default for CdnPolicy {
    fn default() -> Self {
        Self {
            strategy: CdnStrategy::Weighted,
            fallback_to_origin: true,
            sticky_by_region: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: usize,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub retryable_statuses: BTreeSet<u16>,
}

impl RetryPolicy {
    pub fn backoff_for(&self, attempt: usize) -> Duration {
        let multiplier = 2u32.saturating_pow(attempt.saturating_sub(1) as u32);
        let millis = self
            .initial_backoff
            .as_millis()
            .saturating_mul(multiplier as u128);
        let capped = millis.min(self.max_backoff.as_millis());
        Duration::from_millis(capped as u64)
    }

    pub fn should_retry_status(&self, status: u16) -> bool {
        self.retryable_statuses.contains(&status)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        let mut statuses = BTreeSet::new();
        statuses.extend([408, 409, 425, 429, 500, 502, 503, 504]);
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(50),
            max_backoff: Duration::from_millis(500),
            retryable_statuses: statuses,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerPolicy {
    pub failure_threshold: usize,
    pub open_window: Duration,
    pub half_open_permits: usize,
}

impl Default for CircuitBreakerPolicy {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            open_window: Duration::from_secs(10),
            half_open_permits: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthPolicy {
    pub default_scope: String,
    pub required: bool,
    pub forward_trace_headers: bool,
}

impl Default for AuthPolicy {
    fn default() -> Self {
        Self {
            default_scope: "default".to_string(),
            required: false,
            forward_trace_headers: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResumePolicy {
    pub enabled: bool,
    pub chunk_size: usize,
    pub parallelism: usize,
    pub verify_etag: bool,
}

impl Default for ResumePolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            chunk_size: 256 * 1024,
            parallelism: 4,
            verify_etag: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitPolicy {
    pub max_in_flight: usize,
    pub tokens_per_second: usize,
    pub burst: usize,
}

impl Default for RateLimitPolicy {
    fn default() -> Self {
        Self {
            max_in_flight: 128,
            tokens_per_second: 256,
            burst: 512,
        }
    }
}
