use std::fmt::{Display, Formatter};

use crate::error::{NetError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Endpoint {
    scheme: String,
    host: String,
    port: Option<u16>,
    path: String,
    query: Option<String>,
}

impl Endpoint {
    pub fn new(
        scheme: impl Into<String>,
        host: impl Into<String>,
        port: Option<u16>,
        path: impl Into<String>,
    ) -> Self {
        let path = normalize_path(path.into());
        Self {
            scheme: scheme.into(),
            host: host.into(),
            port,
            path,
            query: None,
        }
    }

    pub fn parse(url: &str) -> Result<Self> {
        let (scheme, remain) = url
            .split_once("://")
            .ok_or_else(|| NetError::InvalidRequest(format!("missing scheme in url: {url}")))?;
        let (authority, tail) = match remain.split_once('/') {
            Some((authority, tail)) => (authority, format!("/{tail}")),
            None => (remain, "/".to_string()),
        };
        let (host_port, query) = match tail.split_once('?') {
            Some((path, query)) => (path.to_string(), Some(query.to_string())),
            None => (tail, None),
        };
        let (host, port) = parse_host_port(authority)?;
        Ok(Self {
            scheme: scheme.to_string(),
            host,
            port,
            path: normalize_path(host_port),
            query,
        })
    }

    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> Option<u16> {
        self.port
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    pub fn with_port(mut self, port: Option<u16>) -> Self {
        self.port = port;
        self
    }

    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.query = Some(query.into());
        self
    }

    pub fn authority(&self) -> String {
        match self.port {
            Some(port) => format!("{}:{port}", self.host),
            None => self.host.clone(),
        }
    }

    pub fn origin(&self) -> String {
        format!("{}://{}", self.scheme, self.authority())
    }

    pub fn uri(&self) -> String {
        match &self.query {
            Some(query) => format!("{}?{query}", self.path),
            None => self.path.clone(),
        }
    }

    pub fn join_path(&self, tail: &str) -> Self {
        let mut next = self.clone();
        if next.path.ends_with('/') {
            next.path.push_str(tail.trim_start_matches('/'));
        } else {
            next.path.push('/');
            next.path.push_str(tail.trim_start_matches('/'));
        }
        next
    }
}

impl Display for Endpoint {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}://{}{}", self.scheme, self.authority(), self.uri())
    }
}

fn parse_host_port(authority: &str) -> Result<(String, Option<u16>)> {
    if let Some((host, port)) = authority.rsplit_once(':') {
        if host.contains(']') || port.is_empty() {
            return Ok((authority.to_string(), None));
        }
        match port.parse::<u16>() {
            Ok(port) => Ok((host.to_string(), Some(port))),
            Err(_) => Err(NetError::InvalidRequest(format!(
                "invalid port in authority: {authority}"
            ))),
        }
    } else {
        Ok((authority.to_string(), None))
    }
}

fn normalize_path(path: String) -> String {
    if path.is_empty() {
        "/".to_string()
    } else if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    }
}
