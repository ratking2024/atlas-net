use std::time::Duration;

use crate::config::{
    AuthPolicy, CachePolicy, CdnPolicy, CircuitBreakerPolicy, DnsPolicy, RateLimitPolicy,
    ResumePolicy, RetryPolicy,
};
use crate::types::Headers;

#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    pub service_name: String,
    pub environment: String,
    pub user_agent: String,
    pub request_timeout: Duration,
    pub default_headers: Headers,
    pub retry: RetryPolicy,
    pub circuit_breaker: CircuitBreakerPolicy,
    pub cache: CachePolicy,
    pub dns: DnsPolicy,
    pub cdn: CdnPolicy,
    pub auth: AuthPolicy,
    pub resume: ResumePolicy,
    pub rate_limit: RateLimitPolicy,
}

impl RuntimeOptions {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: service_name.into(),
            ..Self::default()
        }
    }
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            service_name: "atlas-net".to_string(),
            environment: "dev".to_string(),
            user_agent: "atlas-net/0.1.0".to_string(),
            request_timeout: Duration::from_secs(10),
            default_headers: Headers::new().with("accept", "application/json"),
            retry: RetryPolicy::default(),
            circuit_breaker: CircuitBreakerPolicy::default(),
            cache: CachePolicy::default(),
            dns: DnsPolicy::default(),
            cdn: CdnPolicy::default(),
            auth: AuthPolicy::default(),
            resume: ResumePolicy::default(),
            rate_limit: RateLimitPolicy::default(),
        }
    }
}
