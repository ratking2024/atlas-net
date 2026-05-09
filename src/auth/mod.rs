use std::collections::BTreeMap;
use std::sync::Arc;

use crate::config::BusinessContext;
use crate::error::{NetError, Result};
use crate::types::{Headers, Request};

pub trait Signer: Send + Sync {
    fn name(&self) -> &str;
    fn sign(&self, request: &mut Request, context: &BusinessContext) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct ApiKeySigner {
    header_name: String,
    key: String,
}

impl ApiKeySigner {
    pub fn new(header_name: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            header_name: header_name.into(),
            key: key.into(),
        }
    }
}

impl Signer for ApiKeySigner {
    fn name(&self) -> &str {
        "api-key"
    }

    fn sign(&self, request: &mut Request, _context: &BusinessContext) -> Result<()> {
        request
            .headers
            .insert(self.header_name.clone(), self.key.clone());
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct BearerTokenSigner {
    token: String,
}

impl BearerTokenSigner {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

impl Signer for BearerTokenSigner {
    fn name(&self) -> &str {
        "bearer"
    }

    fn sign(&self, request: &mut Request, _context: &BusinessContext) -> Result<()> {
        request
            .headers
            .insert("authorization", format!("Bearer {}", self.token));
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SessionSigner {
    cookie_name: String,
    token: String,
}

impl SessionSigner {
    pub fn new(cookie_name: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            cookie_name: cookie_name.into(),
            token: token.into(),
        }
    }
}

impl Signer for SessionSigner {
    fn name(&self) -> &str {
        "session"
    }

    fn sign(&self, request: &mut Request, _context: &BusinessContext) -> Result<()> {
        let current = request
            .headers
            .get("cookie")
            .unwrap_or_default()
            .to_string();
        let cookie = if current.is_empty() {
            format!("{}={}", self.cookie_name, self.token)
        } else {
            format!("{current}; {}={}", self.cookie_name, self.token)
        };
        request.headers.insert("cookie", cookie);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct HeaderSigner {
    headers: Headers,
}

impl HeaderSigner {
    pub fn new(headers: Headers) -> Self {
        Self { headers }
    }
}

impl Signer for HeaderSigner {
    fn name(&self) -> &str {
        "header"
    }

    fn sign(&self, request: &mut Request, _context: &BusinessContext) -> Result<()> {
        request.headers.extend(&self.headers);
        Ok(())
    }
}

pub struct DelegatingSigner {
    name: String,
    handler: Arc<dyn Fn(&mut Request, &BusinessContext) -> Result<()> + Send + Sync>,
}

impl DelegatingSigner {
    pub fn new(
        name: impl Into<String>,
        handler: impl Fn(&mut Request, &BusinessContext) -> Result<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            handler: Arc::new(handler),
        }
    }
}

impl Signer for DelegatingSigner {
    fn name(&self) -> &str {
        &self.name
    }

    fn sign(&self, request: &mut Request, context: &BusinessContext) -> Result<()> {
        (self.handler)(request, context)
    }
}

#[derive(Default)]
pub struct Authenticator {
    scopes: BTreeMap<String, Vec<Arc<dyn Signer>>>,
}

impl Authenticator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<S>(mut self, scope: impl Into<String>, signer: S) -> Self
    where
        S: Signer + 'static,
    {
        self.push(scope, signer);
        self
    }

    pub fn push<S>(&mut self, scope: impl Into<String>, signer: S)
    where
        S: Signer + 'static,
    {
        self.scopes
            .entry(scope.into())
            .or_default()
            .push(Arc::new(signer));
    }

    pub fn sign(
        &self,
        request: &mut Request,
        context: &BusinessContext,
        scope: Option<&str>,
        required: bool,
    ) -> Result<Vec<String>> {
        let scope = scope.unwrap_or("default");
        let signers = self.scopes.get(scope);
        match signers {
            Some(chain) => {
                let mut names = Vec::with_capacity(chain.len());
                for signer in chain {
                    signer.sign(request, context)?;
                    names.push(signer.name().to_string());
                }
                Ok(names)
            }
            None if required => Err(NetError::Auth(format!(
                "missing auth chain for scope `{scope}`"
            ))),
            None => Ok(Vec::new()),
        }
    }
}
