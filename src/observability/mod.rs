use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use crate::cache::CacheSummary;
use crate::error::{NetError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct ClientEvent {
    pub level: EventLevel,
    pub stage: String,
    pub request_id: String,
    pub message: String,
    pub fields: BTreeMap<String, String>,
}

impl ClientEvent {
    pub fn new(
        level: EventLevel,
        stage: impl Into<String>,
        request_id: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            level,
            stage: stage.into(),
            request_id: request_id.into(),
            message: message.into(),
            fields: BTreeMap::new(),
        }
    }

    pub fn field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }
}

pub trait EventSink: Send + Sync {
    fn record(&self, event: ClientEvent) -> Result<()>;
}

#[derive(Default)]
pub struct MemoryEventSink {
    events: Mutex<Vec<ClientEvent>>,
}

impl MemoryEventSink {
    pub fn snapshot(&self) -> Result<Vec<ClientEvent>> {
        let events = self
            .events
            .lock()
            .map_err(|_| NetError::Transport("event sink mutex poisoned".into()))?;
        Ok(events.clone())
    }
}

impl EventSink for MemoryEventSink {
    fn record(&self, event: ClientEvent) -> Result<()> {
        let mut events = self
            .events
            .lock()
            .map_err(|_| NetError::Transport("event sink mutex poisoned".into()))?;
        events.push(event);
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct ExecutionReport {
    pub request_id: String,
    pub attempts: usize,
    pub auth_chain: Vec<String>,
    pub cache: CacheSummary,
    pub resolved_ip: Option<String>,
    pub cdn_node: Option<String>,
    pub target: Option<String>,
    pub backoffs: Vec<Duration>,
    pub events: Vec<ClientEvent>,
}
