pub mod auth;
pub mod cache;
pub mod cdn;
pub mod client;
pub mod config;
pub mod dns;
pub mod error;
pub mod middleware;
pub mod observability;
pub mod transfer;
pub mod transport;
pub mod types;

mod circuit;

pub use auth::{
    ApiKeySigner, Authenticator, BearerTokenSigner, DelegatingSigner, HeaderSigner, SessionSigner,
    Signer,
};
pub use cache::{CacheLookup, CacheStore, CacheSummary, DiskCache, MemoryCache};
pub use cdn::{CdnDecision, CdnRouter, DirectCdnRouter, EdgeNode, RuleBasedCdnRouter};
pub use client::{AtlasClient, AtlasClientBuilder, ResponseEnvelope};
pub use config::{
    AuthPolicy, BusinessContext, BusinessProfile, CacheMode, CachePolicy, CdnPolicy, CdnStrategy,
    CircuitBreakerPolicy, DnsPolicy, DnsSelectionStrategy, RateLimitPolicy, ResumePolicy,
    RetryPolicy, RuntimeOptions,
};
pub use dns::{DnsRecord, PassthroughResolver, ResolvedAddress, Resolver, StaticResolver};
pub use error::{NetError, Result};
pub use middleware::{
    MetadataMiddleware, Middleware, MiddlewareContext, RequestIdMiddleware,
    StaticHeadersMiddleware, TimeoutMiddleware,
};
pub use observability::{ClientEvent, EventLevel, EventSink, ExecutionReport, MemoryEventSink};
pub use transfer::{
    ByteRange, DownloadOutcome, DownloadRequest, MemoryResumeStore, ResumableTransferManager,
    ResumeCheckpoint, ResumeStore, TransferChunk, TransferSpec, UploadOutcome, UploadRequest,
};
#[cfg(feature = "reqwest-transport")]
pub use transport::ReqwestTransport;
pub use transport::{MockTransport, StaticResponseTransport, Transport};
pub use types::{
    Body, BodyKind, Endpoint, HeaderMap, Headers, HttpMethod, Request, RequestBuilder, Response,
    ResponseMetrics, ResponseProvenance,
};

pub mod prelude {
    #[cfg(feature = "reqwest-transport")]
    pub use crate::ReqwestTransport;
    pub use crate::{
        ApiKeySigner, AtlasClient, AtlasClientBuilder, Authenticator, BearerTokenSigner, Body,
        BusinessContext, BusinessProfile, CacheMode, CachePolicy, CacheStore, DirectCdnRouter,
        DiskCache, DnsPolicy, DownloadRequest, Endpoint, Headers, HttpMethod, MemoryCache,
        MemoryEventSink, MemoryResumeStore, MetadataMiddleware, MockTransport, PassthroughResolver,
        RequestBuilder, RequestIdMiddleware, Response, Result, RetryPolicy, RuntimeOptions,
        StaticHeadersMiddleware, StaticResolver, StaticResponseTransport, TimeoutMiddleware,
        Transport, UploadRequest,
    };
}
