use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::{NetError, Result};
use crate::types::{Endpoint, HttpMethod, Request, Response};

#[derive(Debug, Clone)]
pub struct TransportContext {
    pub attempt: usize,
    pub target: Endpoint,
    pub resolved_ip: Option<String>,
}

pub trait Transport: Send + Sync {
    fn execute(&self, request: &Request, context: &TransportContext) -> Result<Response>;
}

pub struct StaticResponseTransport {
    response: Response,
}

impl StaticResponseTransport {
    pub fn new(response: Response) -> Self {
        Self { response }
    }
}

impl Transport for StaticResponseTransport {
    fn execute(&self, _request: &Request, _context: &TransportContext) -> Result<Response> {
        Ok(self.response.clone())
    }
}

type RouteHandler = Arc<dyn Fn(&Request, &TransportContext) -> Result<Response> + Send + Sync>;

pub struct MockTransport {
    routes: BTreeMap<(HttpMethod, String, String), RouteHandler>,
}

impl MockTransport {
    pub fn new() -> Self {
        Self {
            routes: BTreeMap::new(),
        }
    }

    pub fn route(
        mut self,
        method: HttpMethod,
        host: impl Into<String>,
        path: impl Into<String>,
        handler: impl Fn(&Request, &TransportContext) -> Result<Response> + Send + Sync + 'static,
    ) -> Self {
        self.routes.insert(
            (method, host.into(), normalize_path(path.into())),
            Arc::new(handler),
        );
        self
    }
}

impl Default for MockTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for MockTransport {
    fn execute(&self, request: &Request, context: &TransportContext) -> Result<Response> {
        let key = (
            request.method.clone(),
            context.target.host().to_string(),
            context.target.path().to_string(),
        );
        let handler = self.routes.get(&key).ok_or_else(|| {
            NetError::Transport(format!(
                "no mock route registered for {:?} {}{}",
                request.method,
                context.target.host(),
                context.target.path()
            ))
        })?;
        handler(request, context)
    }
}

fn normalize_path(path: String) -> String {
    if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    }
}

#[cfg(feature = "reqwest-transport")]
mod reqwest_transport {
    use std::net::{IpAddr, SocketAddr};
    use std::str::FromStr;
    use std::time::Duration;

    use reqwest::blocking::Client;
    use reqwest::Method;

    use crate::error::{NetError, Result};
    use crate::transport::{Transport, TransportContext};
    use crate::types::{Body, HttpMethod, Request, Response};

    pub struct ReqwestTransport {
        connect_timeout: Duration,
        pool_idle_timeout: Duration,
        pool_max_idle_per_host: usize,
        accept_invalid_certs: bool,
    }

    impl Default for ReqwestTransport {
        fn default() -> Self {
            Self {
                connect_timeout: Duration::from_secs(5),
                pool_idle_timeout: Duration::from_secs(30),
                pool_max_idle_per_host: 8,
                accept_invalid_certs: false,
            }
        }
    }

    impl ReqwestTransport {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn connect_timeout(mut self, timeout: Duration) -> Self {
            self.connect_timeout = timeout;
            self
        }

        pub fn pool_idle_timeout(mut self, timeout: Duration) -> Self {
            self.pool_idle_timeout = timeout;
            self
        }

        pub fn pool_max_idle_per_host(mut self, limit: usize) -> Self {
            self.pool_max_idle_per_host = limit;
            self
        }

        pub fn accept_invalid_certs(mut self, enabled: bool) -> Self {
            self.accept_invalid_certs = enabled;
            self
        }

        fn build_client(&self, request: &Request, context: &TransportContext) -> Result<Client> {
            let mut builder = Client::builder()
                .connect_timeout(self.connect_timeout)
                .timeout(request.timeout)
                .pool_idle_timeout(self.pool_idle_timeout)
                .pool_max_idle_per_host(self.pool_max_idle_per_host)
                .danger_accept_invalid_certs(self.accept_invalid_certs);

            if let Some(ip) = &context.resolved_ip {
                if let Ok(ip) = IpAddr::from_str(ip) {
                    let port = context.target.port().unwrap_or_else(|| {
                        if context.target.scheme() == "http" {
                            80
                        } else {
                            443
                        }
                    });
                    let socket = SocketAddr::new(ip, port);
                    builder = builder.resolve_to_addrs(context.target.host(), &[socket]);
                }
            }

            builder.build().map_err(|error| {
                NetError::Transport(format!("failed to build reqwest client: {error}"))
            })
        }
    }

    impl Transport for ReqwestTransport {
        fn execute(&self, request: &Request, context: &TransportContext) -> Result<Response> {
            let client = self.build_client(request, context)?;
            let method = map_method(&request.method);
            let mut builder = client.request(method, context.target.to_string());

            for (key, value) in request.headers.iter() {
                builder = builder.header(key.as_str(), value.as_str());
            }

            builder = match &request.body {
                Body::Empty => builder,
                Body::Bytes(bytes) => builder.body(bytes.clone()),
                Body::Text(text) => builder.body(text.clone()),
                Body::Segment { bytes, .. } => builder.body(bytes.clone()),
            };

            let response = builder
                .send()
                .map_err(|error| NetError::Transport(format!("reqwest send failed: {error}")))?;
            let status = response.status().as_u16();
            let mut atlas_response = Response::new(status);
            for (key, value) in response.headers() {
                if let Ok(value) = value.to_str() {
                    atlas_response.headers.insert(key.as_str(), value);
                }
            }
            let bytes = response.bytes().map_err(|error| {
                NetError::Transport(format!("reqwest read body failed: {error}"))
            })?;
            atlas_response.body = Body::Bytes(bytes.to_vec());
            Ok(atlas_response)
        }
    }

    fn map_method(method: &HttpMethod) -> Method {
        match method {
            HttpMethod::Get => Method::GET,
            HttpMethod::Post => Method::POST,
            HttpMethod::Put => Method::PUT,
            HttpMethod::Patch => Method::PATCH,
            HttpMethod::Delete => Method::DELETE,
            HttpMethod::Head => Method::HEAD,
            HttpMethod::Options => Method::OPTIONS,
        }
    }
}

#[cfg(feature = "reqwest-transport")]
pub use reqwest_transport::ReqwestTransport;
