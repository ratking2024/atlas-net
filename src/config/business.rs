use std::collections::BTreeMap;

use crate::types::Headers;

#[derive(Debug, Clone, Default)]
pub struct BusinessContext {
    pub app: String,
    pub operation: String,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub device_id: Option<String>,
    pub region: Option<String>,
    pub channel: Option<String>,
    pub environment: Option<String>,
    pub trace_id: Option<String>,
    pub attributes: BTreeMap<String, String>,
}

impl BusinessContext {
    pub fn new(app: impl Into<String>, operation: impl Into<String>) -> Self {
        Self {
            app: app.into(),
            operation: operation.into(),
            ..Self::default()
        }
    }

    pub fn headers(&self) -> Headers {
        let mut headers = Headers::new();
        if !self.app.is_empty() {
            headers.insert("x-biz-app", self.app.clone());
        }
        if !self.operation.is_empty() {
            headers.insert("x-biz-operation", self.operation.clone());
        }
        if let Some(tenant_id) = &self.tenant_id {
            headers.insert("x-biz-tenant-id", tenant_id.clone());
        }
        if let Some(user_id) = &self.user_id {
            headers.insert("x-biz-user-id", user_id.clone());
        }
        if let Some(device_id) = &self.device_id {
            headers.insert("x-biz-device-id", device_id.clone());
        }
        if let Some(region) = &self.region {
            headers.insert("x-biz-region", region.clone());
        }
        if let Some(channel) = &self.channel {
            headers.insert("x-biz-channel", channel.clone());
        }
        if let Some(environment) = &self.environment {
            headers.insert("x-biz-environment", environment.clone());
        }
        if let Some(trace_id) = &self.trace_id {
            headers.insert("x-trace-id", trace_id.clone());
        }
        for (key, value) in &self.attributes {
            headers.insert(format!("x-biz-attr-{key}"), value.clone());
        }
        headers
    }
}

#[derive(Debug, Clone, Default)]
pub struct BusinessProfile {
    pub auth_scope: Option<String>,
    pub cache_namespace: Option<String>,
    pub route_tags: Vec<String>,
    pub headers: Headers,
    pub metadata: BTreeMap<String, String>,
}

impl BusinessProfile {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn auth_scope(mut self, scope: impl Into<String>) -> Self {
        self.auth_scope = Some(scope.into());
        self
    }

    pub fn cache_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.cache_namespace = Some(namespace.into());
        self
    }

    pub fn route_tag(mut self, tag: impl Into<String>) -> Self {
        self.route_tags.push(tag.into());
        self
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key, value);
        self
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}
