use std::time::Duration;

use atlas_net::prelude::*;
use atlas_net::{CdnPolicy, CdnStrategy, DnsRecord, EdgeNode, ResolvedAddress, RuleBasedCdnRouter};

fn main() -> atlas_net::Result<()> {
    let mut business = BusinessContext::new("merchant-center", "query-orders");
    business.region = Some("cn-east-1".into());
    business.tenant_id = Some("tenant-a".into());
    business.trace_id = Some("trace-001".into());

    let mut options = RuntimeOptions::new("merchant-center");
    options.request_timeout = Duration::from_secs(3);
    options.cdn = CdnPolicy {
        strategy: CdnStrategy::RegionAffinity,
        fallback_to_origin: true,
        sticky_by_region: true,
    };

    let resolver = StaticResolver::new()
        .insert(DnsRecord {
            host: "edge-cn.example.com".into(),
            ttl: Duration::from_secs(30),
            source: "static".into(),
            addresses: vec![ResolvedAddress {
                ip: "10.10.1.10".into(),
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
                ip: "10.10.9.10".into(),
                port: 443,
                region: Some("origin".into()),
                weight: 100,
                is_ipv6: false,
            }],
        });

    let cdn_router = RuleBasedCdnRouter::new(vec![EdgeNode {
        name: "cn-east-edge".into(),
        domain: "edge-cn.example.com".into(),
        region: Some("cn-east-1".into()),
        weight: 100,
        healthy: true,
        tags: vec!["merchant".into(), "orders".into()],
    }]);

    let transport = MockTransport::new().route(
        HttpMethod::Get,
        "edge-cn.example.com",
        "/orders",
        |request, context| {
            let auth = request.headers.get("authorization").unwrap_or_default();
            let tenant = request.headers.get("x-biz-tenant-id").unwrap_or_default();
            Ok(Response::new(200)
                .with_header("content-type", "application/json")
                .with_body(format!(
                    "{{\"target\":\"{}\",\"ip\":\"{}\",\"tenant\":\"{}\",\"auth\":\"{}\"}}",
                    context.target,
                    context.resolved_ip.clone().unwrap_or_default(),
                    tenant,
                    auth
                )))
        },
    );

    let client = AtlasClient::builder("merchant-center")
        .options(options)
        .business_context(business)
        .middleware(RequestIdMiddleware)
        .middleware(StaticHeadersMiddleware::new(
            Headers::new().with("x-client-capability", "cdn,resume,cache"),
        ))
        .middleware(TimeoutMiddleware::new(Duration::from_secs(2)))
        .authenticator(
            Authenticator::new().register("default", BearerTokenSigner::new("token-123")),
        )
        .resolver(resolver)
        .cdn_router(cdn_router)
        .cache(DiskCache::new(
            std::env::temp_dir().join("atlas-net-example-cache"),
        )?)
        .transport(transport)
        .build();

    let request = RequestBuilder::new(
        HttpMethod::Get,
        Endpoint::parse("https://api.example.com/orders")?,
    )
    .business_profile(
        BusinessProfile::new()
            .route_tag("merchant")
            .route_tag("orders")
            .cache_namespace("tenant-a"),
    )
    .build();

    let envelope = client.send(request)?;
    println!("status={}", envelope.response.status);
    println!(
        "body={:?}",
        String::from_utf8_lossy(&envelope.response.body.into_bytes())
    );
    println!("events={}", envelope.report.events.len());
    Ok(())
}
