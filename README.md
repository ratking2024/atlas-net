# Atlas Net

## English Version

`atlas-net` is a Rust networking foundation built in an infrastructure-kernel style. It is not just a thin HTTP wrapper. The goal is to break down the networking concerns that real business systems usually need into clear, composable modules:

- Authentication
- Request caching
- DNS resolution and address selection
- CDN routing and origin fallback
- Retries and circuit breaking
- Resumable and chunked transfer
- Business context injection
- Execution reporting and observability

The project is centered around synchronous traits so the policy stack can be reused with different concrete transports, for example:

- HTTP clients
- TCP/QUIC gateways
- RPC runtimes
- Internal unified proxy layers

## Design Goals

1. **Separate networking capabilities from business policies**
2. **Decouple execution from strategy**
3. **Leave extension points for business customization without making the architecture messy**
4. **Allow transport replacement without rewriting cache/auth/dns/cdn/resume logic**

## Core Capabilities

- `auth`
  - Supports API key, bearer token, session cookie, static headers, and delegated custom signers
- `cache`
  - Supports in-memory cache, disk cache, TTL, stale-if-error, vary headers, and business namespaces
- `dns`
  - Supports static resolution, passthrough resolution, round-robin, region-first, and primary-only strategies
- `cdn`
  - Supports edge node routing by weight, region, and tags, with origin fallback
- `client`
  - Orchestrates default headers, business headers, authentication, cache, CDN, DNS, retries, circuit breaking, and execution reports
- `transfer`
  - Supports chunk planning, resume checkpoints, segmented bodies, and high-level upload/download APIs
- `middleware`
  - Supports request, response, and error phase hooks
- `transport`
  - Supports mock transport, static transport, and real HTTP transport backed by `reqwest`
- `observability`
  - Supports event logging, execution reports, and in-memory event collection

## Project Structure

```text
atlas-net/
├── Cargo.toml
├── README.md
├── examples/
│   └── gateway_client.rs
├── tests/
│   └── pipeline.rs
└── src/
    ├── lib.rs
    ├── error.rs
    ├── client.rs
    ├── circuit.rs
    ├── auth/
    │   └── mod.rs
    ├── cache/
    │   └── mod.rs
    ├── cdn/
    │   └── mod.rs
    ├── config/
    │   ├── mod.rs
    │   ├── business.rs
    │   ├── policy.rs
    │   └── runtime.rs
    ├── dns/
    │   └── mod.rs
    ├── middleware/
    │   └── mod.rs
    ├── observability/
    │   └── mod.rs
    ├── transfer/
    │   └── mod.rs
    ├── transport/
    │   └── mod.rs
    └── types/
        ├── mod.rs
        ├── body.rs
        ├── endpoint.rs
        ├── headers.rs
        ├── request.rs
        └── response.rs
```

## Layering

### 1. `types`

The lowest-level domain model layer:

- `Request`
- `Response`
- `Endpoint`
- `Headers`
- `Body`

This layer tries to stay free of policy logic and only expresses networking objects.

### 2. `config`

The strategy and business-input layer:

- `RuntimeOptions`
- `BusinessContext`
- `BusinessProfile`
- `CachePolicy`
- `DnsPolicy`
- `CdnPolicy`
- `RetryPolicy`
- `CircuitBreakerPolicy`
- `ResumePolicy`

The intended usage is to put system-wide networking defaults into `RuntimeOptions`, then refine behavior per endpoint, tenant, or scenario with `BusinessProfile`.

### 3. `auth` / `cache` / `dns` / `cdn` / `transfer`

Capability modules, each focused on a single concern:

- `auth` only decides how requests are signed
- `cache` only decides how responses are looked up or stored
- `dns` only decides how a host becomes a concrete address
- `cdn` only decides which edge route should be used
- `transfer` only decides how chunking and resume behavior work
- `middleware` only handles cross-cutting request lifecycle logic

### 4. `transport`

Execution adapter layer.

`atlas-net` does not hardcode the underlying protocol implementation. Instead, it exposes a `Transport` trait that owns the actual send step.

That means it can naturally sit on top of:

- `reqwest`
- `hyper`
- private RPC SDKs
- internal API gateway clients

### 5. `client`

Top-level orchestration layer. This is where all capabilities are connected:

1. Inject runtime and business headers
2. Select auth scope and sign the request
3. Check cache
4. Run CDN routing
5. Resolve DNS
6. Send through transport
7. Apply retry and circuit-breaker policies
8. Run response and error middleware
9. Persist cache
10. Produce execution reports

## Business Extension Points

The project intentionally leaves several entry points for real business customization:

- `BusinessContext`
  - For call-scoped context
  - Examples: tenant, user, device, region, trace id, environment
- `BusinessProfile`
  - For per-request behavior
  - Examples: auth scope, cache namespace, route tags, custom headers
- `DelegatingSigner`
  - For custom business-side signing logic
  - Examples: internal gateway signatures, short-lived tickets, gray-release headers
- `Transport`
  - For concrete network runtime integration
  - Examples: HTTP, internal RPC, object storage, download gateways
- `Middleware`
  - For horizontal cross-cutting concerns
  - Examples: request ID injection, standard headers, audit metadata, dynamic timeout, error decoration

## Quick Example

```rust
use atlas_net::prelude::*;

let mut business = BusinessContext::new("merchant-center", "query-order");
business.region = Some("cn-east-1".into());
business.trace_id = Some("trace-001".into());

let resolver = StaticResolver::new().insert(DnsRecord {
    host: "edge.example.com".into(),
    ttl: std::time::Duration::from_secs(30),
    source: "static".into(),
    addresses: vec![ResolvedAddress {
        ip: "10.0.0.10".into(),
        port: 443,
        region: Some("cn-east-1".into()),
        weight: 100,
        is_ipv6: false,
    }],
});

let transport = MockTransport::new().route(
    HttpMethod::Get,
    "edge.example.com",
    "/orders",
    |request, _ctx| {
        let authorized = request.headers.get("authorization").unwrap_or_default();
        Ok(Response::new(200).with_body(format!("authorized={authorized}")))
    },
);

let client = AtlasClient::builder("merchant-center")
    .business_context(business)
    .authenticator(
        Authenticator::new().register("default", BearerTokenSigner::new("token-123")),
    )
    .resolver(resolver)
    .transport(transport)
    .build();

let response = client
    .send(
        RequestBuilder::new(HttpMethod::Get, Endpoint::parse("https://edge.example.com/orders").unwrap())
            .build(),
    )
    .unwrap();

assert_eq!(response.response.status, 200);
```

## Recommended Next Extensions

- Add a Redis cache implementation
- Add real HMAC / RSA / STS signers
- Add rate limiters and connection pooling
- Add request coalescing, deduplication, and single-flight behavior
- Improve upload/download server-side negotiation
- Export metrics to Prometheus / OpenTelemetry

## Current Boundaries

The current version focuses on **architecture, module boundaries, and policy orchestration**, not on replacing a mature production HTTP stack end to end.

Already included:

- Clear layering
- Compilable implementation
- Multi-module collaboration
- Business extension points
- Real `reqwest` transport
- Disk cache
- Middleware and hook pipeline
- High-level upload and download APIs
- Examples and tests

Not built in yet, or still worth improving:

- Real connection pool tuning
- Production-grade cryptographic signing
- Multi-process cache consistency
- Server-side session negotiation protocol for upload/download

These areas should continue to evolve mainly through the `Transport`, `CacheStore`, and `Signer` extension surfaces.

---

## 中文版本

`atlas-net` 是一个偏“基础设施内核”风格的 Rust 网络库工程，目标不是只封一层 HTTP 调用，而是把业务侧真正会遇到的网络能力拆成清晰的模块：

- 鉴权链路
- 请求缓存
- DNS 解析与地址选择
- CDN 路由与回源
- 重试与熔断
- 断点续传与分片传输
- 业务属性透传
- 可观测执行报告

整个项目以同步 trait 为核心，便于后续替换成任意真实 transport 实现，例如：

- HTTP 客户端
- TCP/QUIC 网关
- RPC 框架
- 内部统一代理层

## 设计目标

1. **把“网络能力”和“业务策略”分开**
2. **把“执行层”和“策略层”解耦**
3. **给业务定制留口子，但不让代码结构失控**
4. **即便换 transport，也尽量不动 cache/auth/dns/cdn/resume 逻辑**

## 核心能力

- `auth`
  - 支持 API Key、Bearer Token、Session Cookie、静态 Header、委托式自定义签名器
- `cache`
  - 支持内存缓存、磁盘缓存、TTL、stale-if-error、vary header、业务命名空间
- `dns`
  - 支持静态解析器、直通解析器、轮询/地域优先/主地址优先策略
- `cdn`
  - 支持按权重、地域、tag 路由边缘节点，并保留回源 fallback
- `client`
  - 串联默认头、业务头、鉴权、缓存、CDN、DNS、重试、熔断、执行报告
- `transfer`
  - 支持分片规划、续传 checkpoint、分段 body 封装，以及高级上传/下载 API
- `middleware`
  - 支持 request / response / error 三阶段 hook
- `transport`
  - 支持 mock transport、静态 transport、以及 `reqwest` 真实 HTTP transport
- `observability`
  - 支持事件埋点、执行报告、内存事件收集

## 目录结构

```text
atlas-net/
├── Cargo.toml
├── README.md
├── examples/
│   └── gateway_client.rs
├── tests/
│   └── pipeline.rs
└── src/
    ├── lib.rs
    ├── error.rs
    ├── client.rs
    ├── circuit.rs
    ├── auth/
    │   └── mod.rs
    ├── cache/
    │   └── mod.rs
    ├── cdn/
    │   └── mod.rs
    ├── config/
    │   ├── mod.rs
    │   ├── business.rs
    │   ├── policy.rs
    │   └── runtime.rs
    ├── dns/
    │   └── mod.rs
    ├── middleware/
    │   └── mod.rs
    ├── observability/
    │   └── mod.rs
    ├── transfer/
    │   └── mod.rs
    ├── transport/
    │   └── mod.rs
    └── types/
        ├── mod.rs
        ├── body.rs
        ├── endpoint.rs
        ├── headers.rs
        ├── request.rs
        └── response.rs
```

## 分层说明

### 1. `types`

最底层领域模型：

- `Request`
- `Response`
- `Endpoint`
- `Headers`
- `Body`

这里尽量不掺策略逻辑，只表达网络对象。

### 2. `config`

策略与业务入参层：

- `RuntimeOptions`
- `BusinessContext`
- `BusinessProfile`
- `CachePolicy`
- `DnsPolicy`
- `CdnPolicy`
- `RetryPolicy`
- `CircuitBreakerPolicy`
- `ResumePolicy`

建议业务系统把“默认网络能力”封进 `RuntimeOptions`，再按接口/租户/场景通过 `BusinessProfile` 细化。

### 3. `auth` / `cache` / `dns` / `cdn` / `transfer`

能力模块层，各模块只关心自己职责：

- `auth` 只处理如何对请求签名
- `cache` 只处理如何命中/写入缓存
- `dns` 只处理如何把 host 转成地址
- `cdn` 只处理如何挑边缘节点
- `transfer` 只处理如何做分片和断点续传
- `middleware` 只处理跨请求横切逻辑

### 4. `transport`

执行适配层。

`atlas-net` 自己不把底层网络协议写死，只通过 `Transport` trait 把真正的发送动作抽象出来。

这意味着你后续可以非常自然地接：

- `reqwest`
- `hyper`
- 私有 RPC SDK
- 公司内部 API 网关

### 5. `client`

顶层编排层，负责把所有能力串起来：

1. 注入 runtime / business headers
2. 选择 auth scope 并签名
3. 查缓存
4. 跑 CDN 路由
5. 做 DNS 解析
6. 走 transport 发请求
7. 根据策略重试 / 熔断
8. 执行 response/error middleware
9. 写缓存
10. 产出执行报告

## 业务扩展口

这个项目特意给业务配置留了几种入口：

- `BusinessContext`
  - 面向“调用上下文”
  - 例如：租户、用户、设备、区域、trace id、环境
- `BusinessProfile`
  - 面向“单个请求策略”
  - 例如：auth scope、cache namespace、route tags、自定义 headers
- `DelegatingSigner`
  - 面向“业务自定义签名逻辑”
  - 例如：内部网关签名、短期票据、灰度 header
- `Transport`
  - 面向“真实网络栈”
  - 例如：HTTP、内网 RPC、对象存储、下载网关
- `Middleware`
  - 面向“横切逻辑”
  - 例如：请求 ID、统一 header、审计元信息、动态 timeout、错误修饰

## 快速示例

```rust
use atlas_net::prelude::*;

let mut business = BusinessContext::new("merchant-center", "query-order");
business.region = Some("cn-east-1".into());
business.trace_id = Some("trace-001".into());

let resolver = StaticResolver::new().insert(DnsRecord {
    host: "edge.example.com".into(),
    ttl: std::time::Duration::from_secs(30),
    source: "static".into(),
    addresses: vec![ResolvedAddress {
        ip: "10.0.0.10".into(),
        port: 443,
        region: Some("cn-east-1".into()),
        weight: 100,
        is_ipv6: false,
    }],
});

let transport = MockTransport::new().route(
    HttpMethod::Get,
    "edge.example.com",
    "/orders",
    |request, _ctx| {
        let authorized = request.headers.get("authorization").unwrap_or_default();
        Ok(Response::new(200).with_body(format!("authorized={authorized}")))
    },
);

let client = AtlasClient::builder("merchant-center")
    .business_context(business)
    .authenticator(
        Authenticator::new().register("default", BearerTokenSigner::new("token-123")),
    )
    .resolver(resolver)
    .transport(transport)
    .build();

let response = client
    .send(
        RequestBuilder::new(HttpMethod::Get, Endpoint::parse("https://edge.example.com/orders").unwrap())
            .build(),
    )
    .unwrap();

assert_eq!(response.response.status, 200);
```

## 适合继续扩展的方向

- 增加 Redis 缓存实现
- 增加真正的 HMAC / RSA / STS 签名器
- 增加限流器与连接池
- 增加请求合并、去重和单飞
- 增强下载器 / 上传器的服务端协商能力
- 增加指标导出到 Prometheus / OpenTelemetry

## 当前实现边界

当前版本重点在“工程骨架和策略编排”，不是完整替代成熟 HTTP client。

已经具备：

- 合理分层
- 可编译实现
- 多模块协作
- 扩展口设计
- `reqwest` 真实 transport
- 磁盘缓存
- middleware/hook 机制
- 上传/下载高级 API
- 样例与测试

还未内置或仍可继续增强：

- 真正的连接池复用调优
- 生产级密码学签名
- 多进程缓存一致性
- 上传下载的服务端会话协商协议

这些能力建议后续通过 `Transport`、`CacheStore`、`Signer` 三条扩展面继续深化。
