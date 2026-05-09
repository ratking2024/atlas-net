use std::sync::atomic::{AtomicUsize, Ordering};

use crate::config::{BusinessContext, CdnPolicy, CdnStrategy};
use crate::error::{NetError, Result};
use crate::types::Endpoint;

#[derive(Debug, Clone)]
pub struct EdgeNode {
    pub name: String,
    pub domain: String,
    pub region: Option<String>,
    pub weight: u16,
    pub healthy: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CdnDecision {
    pub selected: Endpoint,
    pub node_name: Option<String>,
    pub fallbacks: Vec<Endpoint>,
    pub rewrite_headers: Vec<(String, String)>,
}

pub trait CdnRouter: Send + Sync {
    fn route(
        &self,
        origin: &Endpoint,
        context: &BusinessContext,
        route_tags: &[String],
        policy: &CdnPolicy,
    ) -> Result<CdnDecision>;
}

#[derive(Default)]
pub struct DirectCdnRouter;

impl CdnRouter for DirectCdnRouter {
    fn route(
        &self,
        origin: &Endpoint,
        _context: &BusinessContext,
        _route_tags: &[String],
        _policy: &CdnPolicy,
    ) -> Result<CdnDecision> {
        Ok(CdnDecision {
            selected: origin.clone(),
            node_name: None,
            fallbacks: Vec::new(),
            rewrite_headers: Vec::new(),
        })
    }
}

pub struct RuleBasedCdnRouter {
    nodes: Vec<EdgeNode>,
    sequence: AtomicUsize,
}

impl RuleBasedCdnRouter {
    pub fn new(nodes: Vec<EdgeNode>) -> Self {
        Self {
            nodes,
            sequence: AtomicUsize::new(0),
        }
    }
}

impl CdnRouter for RuleBasedCdnRouter {
    fn route(
        &self,
        origin: &Endpoint,
        context: &BusinessContext,
        route_tags: &[String],
        policy: &CdnPolicy,
    ) -> Result<CdnDecision> {
        if matches!(policy.strategy, CdnStrategy::Disabled) || self.nodes.is_empty() {
            return DirectCdnRouter.route(origin, context, route_tags, policy);
        }

        let mut candidates = self
            .nodes
            .iter()
            .filter(|node| node.healthy)
            .cloned()
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            if policy.fallback_to_origin {
                return DirectCdnRouter.route(origin, context, route_tags, policy);
            }
            return Err(NetError::Cdn("all cdn nodes are unhealthy".into()));
        }

        if !route_tags.is_empty() {
            candidates.sort_by_key(|node| {
                let matched = route_tags.iter().any(|tag| node.tags.contains(tag));
                if matched {
                    0
                } else {
                    1
                }
            });
        }

        match policy.strategy {
            CdnStrategy::RegionAffinity => {
                if let Some(region) = &context.region {
                    candidates.sort_by_key(|node| {
                        if node.region.as_ref() == Some(region) {
                            0
                        } else {
                            1
                        }
                    });
                }
            }
            CdnStrategy::Weighted => {
                candidates.sort_by_key(|node| std::cmp::Reverse(node.weight));
                let offset = self.sequence.fetch_add(1, Ordering::Relaxed);
                let len = candidates.len();
                candidates.rotate_left(offset % len);
            }
            CdnStrategy::Disabled => {}
        }

        let selected = candidates
            .first()
            .cloned()
            .ok_or_else(|| NetError::Cdn("failed to pick cdn node".into()))?;
        let mut fallbacks = candidates
            .iter()
            .skip(1)
            .map(|node| origin.clone().with_host(node.domain.clone()))
            .collect::<Vec<_>>();
        if policy.fallback_to_origin {
            fallbacks.push(origin.clone());
        }

        Ok(CdnDecision {
            selected: origin.clone().with_host(selected.domain.clone()),
            node_name: Some(selected.name),
            fallbacks,
            rewrite_headers: vec![("x-origin-host".into(), origin.host().to_string())],
        })
    }
}
