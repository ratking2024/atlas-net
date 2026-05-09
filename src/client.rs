use std::sync::Arc;
use std::time::Instant;

use crate::auth::Authenticator;
use crate::cache::{build_cache_key, CacheLookup, CacheStore, MemoryCache};
use crate::cdn::{CdnRouter, DirectCdnRouter};
use crate::circuit::CircuitBreaker;
use crate::config::{BusinessContext, CacheMode, RuntimeOptions};
use crate::dns::{PassthroughResolver, Resolver};
use crate::error::{NetError, Result};
use crate::middleware::{Middleware, MiddlewareContext};
use crate::observability::{ClientEvent, EventLevel, EventSink, ExecutionReport};
use crate::transfer::{
    DownloadOutcome, DownloadRequest, MemoryResumeStore, ResumableTransferManager,
    ResumeCheckpoint, ResumeStore, TransferChunk, TransferSpec, UploadOutcome, UploadRequest,
};
use crate::transport::{StaticResponseTransport, Transport, TransportContext};
use crate::types::{Request, Response};

pub struct ResponseEnvelope {
    pub response: Response,
    pub report: ExecutionReport,
}

pub struct AtlasClient {
    options: RuntimeOptions,
    business: BusinessContext,
    authenticator: Authenticator,
    cache: Arc<dyn CacheStore>,
    resolver: Arc<dyn Resolver>,
    cdn_router: Arc<dyn CdnRouter>,
    transport: Arc<dyn Transport>,
    event_sink: Option<Arc<dyn EventSink>>,
    resume_store: Arc<dyn ResumeStore>,
    middlewares: Vec<Arc<dyn Middleware>>,
    circuit: CircuitBreaker,
    transfer_manager: ResumableTransferManager,
}

impl AtlasClient {
    pub fn builder(service_name: impl Into<String>) -> AtlasClientBuilder {
        AtlasClientBuilder::new(service_name)
    }

    pub fn send(&self, mut request: Request) -> Result<ResponseEnvelope> {
        let mut report = ExecutionReport {
            request_id: request.id.clone(),
            ..ExecutionReport::default()
        };
        let middleware_context = MiddlewareContext {
            request_id: request.id.clone(),
            business: &self.business,
            options: &self.options,
        };

        request.headers.extend(&self.options.default_headers);
        request
            .headers
            .insert("user-agent", self.options.user_agent.clone());
        request.headers.extend(&self.business.headers());
        if let Some(profile) = &request.business_profile {
            request.headers.extend(&profile.headers);
        }
        self.apply_request_middleware(&mut request, &middleware_context, &mut report)?;

        let auth_scope = request
            .auth_scope
            .clone()
            .or_else(|| {
                request
                    .business_profile
                    .as_ref()
                    .and_then(|profile| profile.auth_scope.clone())
            })
            .unwrap_or_else(|| self.options.auth.default_scope.clone());
        report.auth_chain = self.authenticator.sign(
            &mut request,
            &self.business,
            Some(&auth_scope),
            self.options.auth.required,
        )?;
        if !report.auth_chain.is_empty() {
            let chain_text = report.auth_chain.join(",");
            self.push_event(
                &mut report,
                ClientEvent::new(
                    EventLevel::Info,
                    "auth",
                    request.id.clone(),
                    format!("applied auth chain for scope `{auth_scope}`"),
                )
                .field("scope", auth_scope.clone())
                .field("chain", chain_text),
            )?;
        }

        let mut cache_key = None;
        let mut stale_cache = None;
        if request.cacheable() && self.options.cache.enabled() {
            let key = build_cache_key(&request, &self.business, &self.options.cache);
            cache_key = Some(key.clone());
            report.cache.key = Some(key.clone());

            match self.cache.get(&key)? {
                CacheLookup::Hit(mut response) => {
                    if !matches!(self.options.cache.mode, CacheMode::Refresh) {
                        response.metrics.cache_hit = true;
                        response.metrics.attempt = 0;
                        response.provenance.cache_key = Some(key);
                        self.apply_response_middleware(
                            &request,
                            &mut response,
                            &middleware_context,
                            &mut report,
                        )?;
                        report.cache.hit = true;
                        self.push_event(
                            &mut report,
                            ClientEvent::new(
                                EventLevel::Info,
                                "cache",
                                request.id.clone(),
                                "served from cache",
                            ),
                        )?;
                        return Ok(ResponseEnvelope { response, report });
                    }
                }
                CacheLookup::Stale(response) => {
                    stale_cache = Some(response);
                    report.cache.stale = true;
                }
                CacheLookup::Miss => {
                    if matches!(self.options.cache.mode, CacheMode::OnlyIfCached) {
                        return Err(NetError::PolicyViolation(
                            "cache-only mode enabled but cache entry missing".into(),
                        ));
                    }
                }
            }
        }

        let route_tags = request
            .business_profile
            .as_ref()
            .map(|profile| profile.route_tags.clone())
            .unwrap_or_default();
        let cdn = self.cdn_router.route(
            &request.endpoint,
            &self.business,
            &route_tags,
            &self.options.cdn,
        )?;
        for (key, value) in &cdn.rewrite_headers {
            request.headers.insert(key.clone(), value.clone());
        }
        report.cdn_node = cdn.node_name.clone();
        self.push_event(
            &mut report,
            ClientEvent::new(
                EventLevel::Debug,
                "cdn",
                request.id.clone(),
                "cdn route selected",
            )
            .field("target", cdn.selected.to_string()),
        )?;

        let dns = self
            .resolver
            .resolve(&cdn.selected, &self.business, &self.options.dns)?;
        let max_attempts = self.options.retry.max_attempts.max(1);
        let mut last_error = None;

        for attempt in 1..=max_attempts {
            self.circuit.allow()?;
            let chosen_address = dns
                .addresses
                .get((attempt - 1).min(dns.addresses.len() - 1))
                .cloned()
                .ok_or_else(|| NetError::Dns("dns returned no usable addresses".into()))?;
            report.resolved_ip = Some(chosen_address.ip.clone());
            report.target = Some(cdn.selected.to_string());

            self.push_event(
                &mut report,
                ClientEvent::new(
                    EventLevel::Debug,
                    "attempt",
                    request.id.clone(),
                    format!("starting attempt {attempt}"),
                )
                .field("ip", chosen_address.ip.clone()),
            )?;

            let start = Instant::now();
            let context = TransportContext {
                attempt,
                target: cdn.selected.clone().with_port(Some(chosen_address.port)),
                resolved_ip: Some(chosen_address.ip.clone()),
            };

            match self.transport.execute(&request, &context) {
                Ok(mut response) => {
                    response.provenance.original_endpoint = Some(request.endpoint.clone());
                    response.provenance.selected_endpoint = Some(context.target.clone());
                    response.provenance.resolved_ip = context.resolved_ip.clone();
                    response.provenance.cdn_node = cdn.node_name.clone();
                    response.provenance.cache_key = cache_key.clone();
                    response.metrics.attempt = attempt;
                    response.metrics.latency = start.elapsed();
                    response.metrics.bytes_out = request.payload_len();
                    response.metrics.bytes_in = response.body.len();
                    report.attempts = attempt;
                    self.apply_response_middleware(
                        &request,
                        &mut response,
                        &middleware_context,
                        &mut report,
                    )?;

                    if self.options.retry.should_retry_status(response.status)
                        && attempt < max_attempts
                    {
                        self.circuit.on_failure()?;
                        let backoff = self.options.retry.backoff_for(attempt);
                        report.backoffs.push(backoff);
                        self.push_event(
                            &mut report,
                            ClientEvent::new(
                                EventLevel::Warn,
                                "retry",
                                request.id.clone(),
                                format!("retrying due to status {}", response.status),
                            )
                            .field("backoff_ms", backoff.as_millis().to_string()),
                        )?;
                        last_error = Some(NetError::Transport(format!(
                            "retryable response status {}",
                            response.status
                        )));
                        continue;
                    }

                    if response.is_success() {
                        self.circuit.on_success()?;
                        if let Some(key) = &cache_key {
                            if self.options.cache.should_store_status(response.status) {
                                self.cache.put(
                                    key.clone(),
                                    response.clone(),
                                    self.options.cache.default_ttl,
                                    self.options.cache.stale_if_error,
                                )?;
                            }
                        }
                    } else {
                        self.circuit.on_failure()?;
                    }

                    self.push_event(
                        &mut report,
                        ClientEvent::new(
                            EventLevel::Info,
                            "response",
                            request.id.clone(),
                            format!("received status {}", response.status),
                        )
                        .field("attempt", attempt.to_string()),
                    )?;

                    return Ok(ResponseEnvelope { response, report });
                }
                Err(mut error) => {
                    self.apply_error_middleware(
                        &request,
                        &mut error,
                        &middleware_context,
                        &mut report,
                    )?;
                    self.circuit.on_failure()?;
                    self.push_event(
                        &mut report,
                        ClientEvent::new(
                            EventLevel::Warn,
                            "transport",
                            request.id.clone(),
                            format!("attempt {attempt} failed: {error}"),
                        ),
                    )?;

                    if attempt < max_attempts && error.is_retryable() {
                        let backoff = self.options.retry.backoff_for(attempt);
                        report.backoffs.push(backoff);
                        last_error = Some(error);
                        continue;
                    }

                    if let Some(mut stale) = stale_cache.clone() {
                        stale.metrics.cache_hit = true;
                        stale.provenance.cache_key = cache_key.clone();
                        report.cache.hit = true;
                        report.cache.stale = true;
                        self.push_event(
                            &mut report,
                            ClientEvent::new(
                                EventLevel::Warn,
                                "cache",
                                request.id.clone(),
                                "falling back to stale cache",
                            ),
                        )?;
                        return Ok(ResponseEnvelope {
                            response: stale,
                            report,
                        });
                    }

                    return Err(match last_error.take() {
                        Some(previous) => NetError::retry_exhausted(attempt, previous),
                        None => error,
                    });
                }
            }
        }

        Err(NetError::retry_exhausted(
            max_attempts,
            last_error.unwrap_or_else(|| NetError::Transport("request execution aborted".into())),
        ))
    }

    pub fn download(&self, request: DownloadRequest) -> Result<DownloadOutcome> {
        let chunks = self.plan_transfer(&request.spec)?;
        let mut bytes = Vec::new();
        let mut checkpoints = Vec::new();
        let mut final_response = None;

        for chunk in &chunks {
            let range_header = format!("bytes={}-{}", chunk.range.start, chunk.range.end - 1);
            let mut segment_request = request.request.clone();
            segment_request.resumable = true;
            segment_request.headers.insert("range", range_header);
            segment_request
                .headers
                .insert("x-transfer-id", request.spec.transfer_id.clone());

            let envelope = self.send(segment_request)?;
            bytes.extend_from_slice(&envelope.response.body.clone().into_bytes());
            checkpoints.push(self.save_transfer_progress(&request.spec, chunk)?);
            final_response = Some(envelope.response);
        }

        self.clear_transfer_progress(&request.spec.transfer_id)?;
        Ok(DownloadOutcome {
            bytes,
            chunks,
            checkpoints,
            final_response: final_response.unwrap_or_else(|| Response::new(204)),
        })
    }

    pub fn upload(&self, request: UploadRequest) -> Result<UploadOutcome> {
        let chunks = self.plan_transfer(&request.spec)?;
        let mut checkpoints = Vec::new();
        let mut final_response = None;

        for chunk in &chunks {
            let start = chunk.range.start as usize;
            let end = chunk.range.end as usize;
            let body = self.transfer_manager.build_segment_body(
                chunk,
                request.bytes[start..end].to_vec(),
                request.spec.total_size,
            );
            let mut segment_request = request.request.clone();
            segment_request.resumable = true;
            segment_request.body = body;
            segment_request.headers.insert(
                "content-range",
                format!(
                    "bytes {}-{}/{}",
                    chunk.range.start,
                    chunk.range.end.saturating_sub(1),
                    request.spec.total_size
                ),
            );
            segment_request
                .headers
                .insert("x-transfer-id", request.spec.transfer_id.clone());
            if let Some(content_type) = &request.spec.content_type {
                segment_request
                    .headers
                    .insert("content-type", content_type.clone());
            }

            let envelope = self.send(segment_request)?;
            checkpoints.push(self.save_transfer_progress(&request.spec, chunk)?);
            final_response = Some(envelope.response);
        }

        self.clear_transfer_progress(&request.spec.transfer_id)?;
        Ok(UploadOutcome {
            uploaded_bytes: request.bytes.len(),
            chunks,
            checkpoints,
            final_response: final_response.unwrap_or_else(|| Response::new(204)),
        })
    }

    pub fn plan_transfer(&self, spec: &TransferSpec) -> Result<Vec<TransferChunk>> {
        let checkpoint = self.resume_store.load(&spec.transfer_id)?;
        Ok(self.transfer_manager.plan(spec, checkpoint.as_ref()))
    }

    pub fn save_transfer_progress(
        &self,
        spec: &TransferSpec,
        chunk: &TransferChunk,
    ) -> Result<ResumeCheckpoint> {
        let previous = self.resume_store.load(&spec.transfer_id)?;
        let checkpoint = self
            .transfer_manager
            .checkpoint_after(spec, chunk, previous.as_ref());
        self.resume_store.save(checkpoint.clone())?;
        Ok(checkpoint)
    }

    pub fn clear_transfer_progress(&self, transfer_id: &str) -> Result<()> {
        self.resume_store.clear(transfer_id)
    }

    fn push_event(&self, report: &mut ExecutionReport, event: ClientEvent) -> Result<()> {
        if let Some(sink) = &self.event_sink {
            sink.record(event.clone())?;
        }
        report.events.push(event);
        Ok(())
    }

    fn apply_request_middleware(
        &self,
        request: &mut Request,
        context: &MiddlewareContext<'_>,
        report: &mut ExecutionReport,
    ) -> Result<()> {
        for middleware in &self.middlewares {
            middleware.on_request(request, context)?;
            self.push_event(
                report,
                ClientEvent::new(
                    EventLevel::Debug,
                    "middleware.request",
                    request.id.clone(),
                    format!("middleware `{}` applied to request", middleware.name()),
                ),
            )?;
        }
        Ok(())
    }

    fn apply_response_middleware(
        &self,
        request: &Request,
        response: &mut Response,
        context: &MiddlewareContext<'_>,
        report: &mut ExecutionReport,
    ) -> Result<()> {
        for middleware in &self.middlewares {
            middleware.on_response(request, response, context)?;
            self.push_event(
                report,
                ClientEvent::new(
                    EventLevel::Debug,
                    "middleware.response",
                    request.id.clone(),
                    format!("middleware `{}` applied to response", middleware.name()),
                ),
            )?;
        }
        Ok(())
    }

    fn apply_error_middleware(
        &self,
        request: &Request,
        error: &mut NetError,
        context: &MiddlewareContext<'_>,
        report: &mut ExecutionReport,
    ) -> Result<()> {
        for middleware in &self.middlewares {
            middleware.on_error(request, error, context)?;
            self.push_event(
                report,
                ClientEvent::new(
                    EventLevel::Debug,
                    "middleware.error",
                    request.id.clone(),
                    format!("middleware `{}` observed error", middleware.name()),
                ),
            )?;
        }
        Ok(())
    }
}

pub struct AtlasClientBuilder {
    options: RuntimeOptions,
    business: Option<BusinessContext>,
    authenticator: Option<Authenticator>,
    cache: Option<Arc<dyn CacheStore>>,
    resolver: Option<Arc<dyn Resolver>>,
    cdn_router: Option<Arc<dyn CdnRouter>>,
    transport: Option<Arc<dyn Transport>>,
    event_sink: Option<Arc<dyn EventSink>>,
    resume_store: Option<Arc<dyn ResumeStore>>,
    middlewares: Vec<Arc<dyn Middleware>>,
}

impl AtlasClientBuilder {
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            options: RuntimeOptions::new(service_name.into()),
            business: None,
            authenticator: None,
            cache: None,
            resolver: None,
            cdn_router: None,
            transport: None,
            event_sink: None,
            resume_store: None,
            middlewares: Vec::new(),
        }
    }

    pub fn options(mut self, options: RuntimeOptions) -> Self {
        self.options = options;
        self
    }

    pub fn business_context(mut self, business: BusinessContext) -> Self {
        self.business = Some(business);
        self
    }

    pub fn authenticator(mut self, authenticator: Authenticator) -> Self {
        self.authenticator = Some(authenticator);
        self
    }

    pub fn cache<T>(mut self, cache: T) -> Self
    where
        T: CacheStore + 'static,
    {
        self.cache = Some(Arc::new(cache));
        self
    }

    pub fn resolver<T>(mut self, resolver: T) -> Self
    where
        T: Resolver + 'static,
    {
        self.resolver = Some(Arc::new(resolver));
        self
    }

    pub fn cdn_router<T>(mut self, router: T) -> Self
    where
        T: CdnRouter + 'static,
    {
        self.cdn_router = Some(Arc::new(router));
        self
    }

    pub fn transport<T>(mut self, transport: T) -> Self
    where
        T: Transport + 'static,
    {
        self.transport = Some(Arc::new(transport));
        self
    }

    pub fn event_sink<T>(mut self, sink: T) -> Self
    where
        T: EventSink + 'static,
    {
        self.event_sink = Some(Arc::new(sink));
        self
    }

    pub fn resume_store<T>(mut self, store: T) -> Self
    where
        T: ResumeStore + 'static,
    {
        self.resume_store = Some(Arc::new(store));
        self
    }

    pub fn middleware<T>(mut self, middleware: T) -> Self
    where
        T: Middleware + 'static,
    {
        self.middlewares.push(Arc::new(middleware));
        self
    }

    pub fn build(self) -> AtlasClient {
        let circuit = CircuitBreaker::new(self.options.circuit_breaker.clone());
        let transfer_manager = ResumableTransferManager::new(self.options.resume.clone());
        let cache = self
            .cache
            .unwrap_or_else(|| Arc::new(MemoryCache::new(self.options.cache.max_entries)));
        let resolver = self
            .resolver
            .unwrap_or_else(|| Arc::new(PassthroughResolver));
        let cdn_router = self.cdn_router.unwrap_or_else(|| Arc::new(DirectCdnRouter));
        let transport = self
            .transport
            .unwrap_or_else(|| Arc::new(StaticResponseTransport::new(Response::new(204))));

        AtlasClient {
            business: self.business.unwrap_or_else(|| {
                BusinessContext::new(self.options.service_name.clone(), "default")
            }),
            authenticator: self.authenticator.unwrap_or_else(Authenticator::new),
            cache,
            resolver,
            cdn_router,
            transport,
            event_sink: self.event_sink,
            resume_store: self
                .resume_store
                .unwrap_or_else(|| Arc::new(MemoryResumeStore::default())),
            middlewares: self.middlewares,
            circuit,
            transfer_manager,
            options: self.options,
        }
    }
}
