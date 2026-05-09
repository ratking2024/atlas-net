use std::time::Duration;

use crate::types::{Body, Endpoint, Headers};

#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub headers: Headers,
    pub body: Body,
    pub provenance: ResponseProvenance,
    pub metrics: ResponseMetrics,
}

impl Response {
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: Headers::new(),
            body: Body::Empty,
            provenance: ResponseProvenance::default(),
            metrics: ResponseMetrics::default(),
        }
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key, value);
        self
    }

    pub fn with_body(mut self, body: impl Into<Body>) -> Self {
        self.body = body.into();
        self
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResponseProvenance {
    pub original_endpoint: Option<Endpoint>,
    pub selected_endpoint: Option<Endpoint>,
    pub resolved_ip: Option<String>,
    pub cdn_node: Option<String>,
    pub cache_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResponseMetrics {
    pub attempt: usize,
    pub latency: Duration,
    pub bytes_in: usize,
    pub bytes_out: usize,
    pub cache_hit: bool,
}

impl Default for ResponseMetrics {
    fn default() -> Self {
        Self {
            attempt: 1,
            latency: Duration::ZERO,
            bytes_in: 0,
            bytes_out: 0,
            cache_hit: false,
        }
    }
}
