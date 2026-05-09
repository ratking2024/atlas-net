use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use atlas_net::prelude::*;
use atlas_net::{
    CdnPolicy, CdnStrategy, DnsRecord, EdgeNode, ResolvedAddress, RuleBasedCdnRouter, TransferSpec,
};
use atlas_net::{Middleware, MiddlewareContext, NetError, Request, Response};

fn build_resolver() -> StaticResolver {
    StaticResolver::new()
        .insert(DnsRecord {
            host: "edge-cn.example.com".into(),
            ttl: Duration::from_secs(30),
            source: "static".into(),
            addresses: vec![ResolvedAddress {
                ip: "10.0.0.10".into(),
                port: 443,
                region: Some("cn-east-1".into()),
                weight: 100,
                is_ipv6: false,
            }],
        })
        .insert(DnsRecord {
            host: "api.example.com".into(),
            ttl: Duration::from_secs(30),
            source: "static".into(),
            addresses: vec![ResolvedAddress {
                ip: "10.0.0.20".into(),
                port: 443,
                region: Some("origin".into()),
                weight: 100,
                is_ipv6: false,
            }],
        })
}

#[test]
fn pipeline_supports_auth_cache_dns_and_cdn() {
    let calls = Arc::new(AtomicUsize::new(0));
    let transport_calls = calls.clone();

    let transport = MockTransport::new().route(
        HttpMethod::Get,
        "edge-cn.example.com",
        "/orders",
        move |request, context| {
            transport_calls.fetch_add(1, Ordering::SeqCst);
            let token = request.headers.get("authorization").unwrap_or_default();
            Ok(Response::new(200).with_body(format!(
                "token={token};target={};ip={}",
                context.target,
                context.resolved_ip.clone().unwrap_or_default()
            )))
        },
    );

    let mut runtime = RuntimeOptions::new("merchant-center");
    runtime.cdn = CdnPolicy {
        strategy: CdnStrategy::RegionAffinity,
        fallback_to_origin: true,
        sticky_by_region: true,
    };

    let mut business = BusinessContext::new("merchant-center", "query-orders");
    business.region = Some("cn-east-1".into());

    let client = AtlasClient::builder("merchant-center")
        .options(runtime)
        .business_context(business)
        .authenticator(
            Authenticator::new().register("default", BearerTokenSigner::new("token-abc")),
        )
        .resolver(build_resolver())
        .cdn_router(RuleBasedCdnRouter::new(vec![EdgeNode {
            name: "cn-edge".into(),
            domain: "edge-cn.example.com".into(),
            region: Some("cn-east-1".into()),
            weight: 100,
            healthy: true,
            tags: vec!["orders".into()],
        }]))
        .transport(transport)
        .build();

    let request = RequestBuilder::new(
        HttpMethod::Get,
        Endpoint::parse("https://api.example.com/orders").unwrap(),
    )
    .business_profile(BusinessProfile::new().route_tag("orders"))
    .build();

    let first = client.send(request.clone()).unwrap();
    let second = client.send(request).unwrap();

    assert_eq!(first.response.status, 200);
    assert!(String::from_utf8_lossy(&first.response.body.into_bytes()).contains("token-abc"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(second.report.cache.hit);
}

#[test]
fn transfer_manager_plans_resume_chunks() {
    let client = AtlasClient::builder("downloader").build();
    let spec = TransferSpec {
        transfer_id: "file-a".into(),
        total_size: 1024 * 1024,
        etag: Some("etag-1".into()),
        content_type: Some("application/octet-stream".into()),
        business_tags: vec!["video".into()],
    };

    let chunks = client.plan_transfer(&spec).unwrap();
    assert!(!chunks.is_empty());

    let checkpoint = client.save_transfer_progress(&spec, &chunks[0]).unwrap();
    assert_eq!(checkpoint.next_offset, chunks[0].range.end);

    let remaining = client.plan_transfer(&spec).unwrap();
    assert!(remaining[0].range.start >= checkpoint.next_offset);
}

#[test]
fn disk_cache_persists_between_instances() {
    let root = std::env::temp_dir().join(format!(
        "atlas-net-cache-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let cache = DiskCache::new(&root).unwrap();
    let key = "orders|demo";
    cache
        .put(
            key.to_string(),
            Response::new(200).with_body("hello"),
            Duration::from_secs(30),
            Duration::from_secs(10),
        )
        .unwrap();
    let another = DiskCache::new(&root).unwrap();
    let found = another.get(key).unwrap();
    match found {
        atlas_net::CacheLookup::Hit(response) => {
            assert_eq!(
                String::from_utf8_lossy(&response.body.into_bytes()),
                "hello"
            );
        }
        _ => panic!("expected persisted cache hit"),
    }
}

#[test]
fn middleware_and_transfer_apis_work_together() {
    struct ResponseHeaderMiddleware;

    impl Middleware for ResponseHeaderMiddleware {
        fn name(&self) -> &str {
            "response-header"
        }

        fn on_request(
            &self,
            request: &mut Request,
            _context: &MiddlewareContext<'_>,
        ) -> atlas_net::Result<()> {
            request.headers.insert("x-middleware", "on");
            Ok(())
        }

        fn on_response(
            &self,
            _request: &Request,
            response: &mut Response,
            _context: &MiddlewareContext<'_>,
        ) -> atlas_net::Result<()> {
            response.headers.insert("x-processed-by", "middleware");
            Ok(())
        }

        fn on_error(
            &self,
            _request: &Request,
            error: &mut NetError,
            _context: &MiddlewareContext<'_>,
        ) -> atlas_net::Result<()> {
            if let NetError::Transport(message) = error {
                *message = format!("wrapped:{message}");
            }
            Ok(())
        }
    }

    let transport = MockTransport::new()
        .route(
            HttpMethod::Get,
            "files.example.com",
            "/blob",
            |request, _context| {
                assert_eq!(request.headers.get("x-middleware"), Some("on"));
                let range = request.headers.get("range").unwrap_or_default();
                if let Some((start, end)) = range
                    .strip_prefix("bytes=")
                    .and_then(|raw| raw.split_once('-'))
                {
                    let start = start.parse::<usize>().unwrap();
                    let end = end.parse::<usize>().unwrap();
                    let payload = vec![b'a'; end - start + 1];
                    Ok(Response::new(206).with_body(payload))
                } else {
                    Ok(Response::new(200).with_body(vec![b'a'; 4]))
                }
            },
        )
        .route(
            HttpMethod::Put,
            "upload.example.com",
            "/blob",
            |_request, _context| Ok(Response::new(201).with_body("uploaded")),
        );

    let client = AtlasClient::builder("asset-center")
        .middleware(ResponseHeaderMiddleware)
        .transport(transport)
        .build();

    let download = client
        .download(atlas_net::DownloadRequest {
            request: RequestBuilder::new(
                HttpMethod::Get,
                Endpoint::parse("https://files.example.com/blob").unwrap(),
            )
            .resumable(true)
            .build(),
            spec: TransferSpec {
                transfer_id: "blob-a".into(),
                total_size: 8,
                etag: Some("etag-a".into()),
                content_type: Some("application/octet-stream".into()),
                business_tags: vec!["download".into()],
            },
        })
        .unwrap();
    assert_eq!(download.bytes.len(), 8);
    assert_eq!(
        download.final_response.headers.get("x-processed-by"),
        Some("middleware")
    );

    let upload = client
        .upload(atlas_net::UploadRequest {
            request: RequestBuilder::new(
                HttpMethod::Put,
                Endpoint::parse("https://upload.example.com/blob").unwrap(),
            )
            .resumable(true)
            .build(),
            spec: TransferSpec {
                transfer_id: "blob-b".into(),
                total_size: 8,
                etag: Some("etag-b".into()),
                content_type: Some("application/octet-stream".into()),
                business_tags: vec!["upload".into()],
            },
            bytes: vec![1, 2, 3, 4, 5, 6, 7, 8],
        })
        .unwrap();
    assert_eq!(upload.uploaded_bytes, 8);
    assert_eq!(upload.final_response.status, 201);
}
