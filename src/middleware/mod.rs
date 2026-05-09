use std::collections::BTreeMap;
use std::time::Duration;

use crate::config::{BusinessContext, RuntimeOptions};
use crate::error::{NetError, Result};
use crate::types::{Headers, Request, Response};

pub struct MiddlewareContext<'a> {
    pub request_id: String,
    pub business: &'a BusinessContext,
    pub options: &'a RuntimeOptions,
}

pub trait Middleware: Send + Sync {
    fn name(&self) -> &str;

    fn on_request(&self, _request: &mut Request, _context: &MiddlewareContext<'_>) -> Result<()> {
        Ok(())
    }

    fn on_response(
        &self,
        _request: &Request,
        _response: &mut Response,
        _context: &MiddlewareContext<'_>,
    ) -> Result<()> {
        Ok(())
    }

    fn on_error(
        &self,
        _request: &Request,
        _error: &mut NetError,
        _context: &MiddlewareContext<'_>,
    ) -> Result<()> {
        Ok(())
    }
}

pub struct StaticHeadersMiddleware {
    headers: Headers,
}

impl StaticHeadersMiddleware {
    pub fn new(headers: Headers) -> Self {
        Self { headers }
    }
}

impl Middleware for StaticHeadersMiddleware {
    fn name(&self) -> &str {
        "static-headers"
    }

    fn on_request(&self, request: &mut Request, _context: &MiddlewareContext<'_>) -> Result<()> {
        request.headers.extend(&self.headers);
        Ok(())
    }
}

pub struct TimeoutMiddleware {
    timeout: Duration,
}

impl TimeoutMiddleware {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl Middleware for TimeoutMiddleware {
    fn name(&self) -> &str {
        "timeout"
    }

    fn on_request(&self, request: &mut Request, _context: &MiddlewareContext<'_>) -> Result<()> {
        request.timeout = self.timeout;
        Ok(())
    }
}

pub struct MetadataMiddleware {
    entries: BTreeMap<String, String>,
}

impl MetadataMiddleware {
    pub fn new(entries: BTreeMap<String, String>) -> Self {
        Self { entries }
    }
}

impl Middleware for MetadataMiddleware {
    fn name(&self) -> &str {
        "metadata"
    }

    fn on_request(&self, request: &mut Request, _context: &MiddlewareContext<'_>) -> Result<()> {
        for (key, value) in &self.entries {
            request.metadata.insert(key.clone(), value.clone());
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct RequestIdMiddleware;

impl Middleware for RequestIdMiddleware {
    fn name(&self) -> &str {
        "request-id"
    }

    fn on_request(&self, request: &mut Request, context: &MiddlewareContext<'_>) -> Result<()> {
        if !request.headers.contains_key("x-request-id") {
            request
                .headers
                .insert("x-request-id", context.request_id.clone());
        }
        Ok(())
    }
}
