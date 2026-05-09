use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::config::BusinessProfile;
use crate::types::{Body, Endpoint, Headers};

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub id: String,
    pub method: HttpMethod,
    pub endpoint: Endpoint,
    pub headers: Headers,
    pub body: Body,
    pub timeout: Duration,
    pub metadata: BTreeMap<String, String>,
    pub business_profile: Option<BusinessProfile>,
    pub auth_scope: Option<String>,
    pub resumable: bool,
}

impl Request {
    pub fn builder(method: HttpMethod, endpoint: Endpoint) -> RequestBuilder {
        RequestBuilder::new(method, endpoint)
    }

    pub fn cacheable(&self) -> bool {
        matches!(self.method, HttpMethod::Get | HttpMethod::Head)
    }

    pub fn payload_len(&self) -> usize {
        self.body.len()
    }
}

#[derive(Debug, Clone)]
pub struct RequestBuilder {
    request: Request,
}

impl RequestBuilder {
    pub fn new(method: HttpMethod, endpoint: Endpoint) -> Self {
        Self {
            request: Request {
                id: next_request_id(),
                method,
                endpoint,
                headers: Headers::new(),
                body: Body::Empty,
                timeout: Duration::from_secs(10),
                metadata: BTreeMap::new(),
                business_profile: None,
                auth_scope: None,
                resumable: false,
            },
        }
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.request.headers.insert(key, value);
        self
    }

    pub fn headers(mut self, headers: Headers) -> Self {
        self.request.headers.extend(&headers);
        self
    }

    pub fn body(mut self, body: impl Into<Body>) -> Self {
        self.request.body = body.into();
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.request.timeout = timeout;
        self
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.request.metadata.insert(key.into(), value.into());
        self
    }

    pub fn business_profile(mut self, profile: BusinessProfile) -> Self {
        self.request.business_profile = Some(profile);
        self
    }

    pub fn auth_scope(mut self, scope: impl Into<String>) -> Self {
        self.request.auth_scope = Some(scope.into());
        self
    }

    pub fn resumable(mut self, enabled: bool) -> Self {
        self.request.resumable = enabled;
        self
    }

    pub fn build(self) -> Request {
        self.request
    }
}

fn next_request_id() -> String {
    let seq = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("req-{seq:010}")
}
