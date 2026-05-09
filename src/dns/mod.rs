use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::config::{BusinessContext, DnsPolicy, DnsSelectionStrategy};
use crate::error::{NetError, Result};
use crate::types::Endpoint;

#[derive(Debug, Clone)]
pub struct ResolvedAddress {
    pub ip: String,
    pub port: u16,
    pub region: Option<String>,
    pub weight: u16,
    pub is_ipv6: bool,
}

#[derive(Debug, Clone)]
pub struct DnsRecord {
    pub host: String,
    pub ttl: Duration,
    pub source: String,
    pub addresses: Vec<ResolvedAddress>,
}

pub trait Resolver: Send + Sync {
    fn resolve(
        &self,
        endpoint: &Endpoint,
        context: &BusinessContext,
        policy: &DnsPolicy,
    ) -> Result<DnsRecord>;
}

#[derive(Default)]
pub struct PassthroughResolver;

impl Resolver for PassthroughResolver {
    fn resolve(
        &self,
        endpoint: &Endpoint,
        context: &BusinessContext,
        policy: &DnsPolicy,
    ) -> Result<DnsRecord> {
        Ok(DnsRecord {
            host: endpoint.host().to_string(),
            ttl: policy.ttl_floor,
            source: "passthrough".into(),
            addresses: vec![ResolvedAddress {
                ip: endpoint.host().to_string(),
                port: endpoint.port().unwrap_or(443),
                region: context.region.clone(),
                weight: 100,
                is_ipv6: endpoint.host().contains(':'),
            }],
        })
    }
}

pub struct StaticResolver {
    table: BTreeMap<String, DnsRecord>,
    sequence: AtomicUsize,
}

impl StaticResolver {
    pub fn new() -> Self {
        Self {
            table: BTreeMap::new(),
            sequence: AtomicUsize::new(0),
        }
    }

    pub fn insert(mut self, record: DnsRecord) -> Self {
        self.table.insert(record.host.clone(), record);
        self
    }

    pub fn push(&mut self, record: DnsRecord) {
        self.table.insert(record.host.clone(), record);
    }
}

impl Default for StaticResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl Resolver for StaticResolver {
    fn resolve(
        &self,
        endpoint: &Endpoint,
        context: &BusinessContext,
        policy: &DnsPolicy,
    ) -> Result<DnsRecord> {
        let mut record = self.table.get(endpoint.host()).cloned().ok_or_else(|| {
            NetError::Dns(format!(
                "no dns record found for host `{}`",
                endpoint.host()
            ))
        })?;

        if record.addresses.is_empty() {
            return Err(NetError::Dns(format!(
                "dns record for host `{}` has no addresses",
                endpoint.host()
            )));
        }

        if policy.prefer_ipv6 {
            record
                .addresses
                .sort_by_key(|address| if address.is_ipv6 { 0 } else { 1 });
        }

        match policy.strategy {
            DnsSelectionStrategy::RoundRobin => {
                let offset = self.sequence.fetch_add(1, Ordering::Relaxed);
                let len = record.addresses.len();
                record.addresses.rotate_left(offset % len);
            }
            DnsSelectionStrategy::RegionFirst => {
                if let Some(region) = &context.region {
                    record.addresses.sort_by_key(|address| {
                        if address.region.as_ref() == Some(region) {
                            0
                        } else {
                            1
                        }
                    });
                }
            }
            DnsSelectionStrategy::PrimaryOnly => {
                let primary = record.addresses[0].clone();
                record.addresses = vec![primary];
            }
        }

        if record.ttl < policy.ttl_floor {
            record.ttl = policy.ttl_floor;
        }

        Ok(record)
    }
}
