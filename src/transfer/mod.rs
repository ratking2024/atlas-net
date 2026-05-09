use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::config::ResumePolicy;
use crate::error::{NetError, Result};
use crate::types::{Body, Request, Response};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}

impl ByteRange {
    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }
}

#[derive(Debug, Clone)]
pub struct TransferSpec {
    pub transfer_id: String,
    pub total_size: u64,
    pub etag: Option<String>,
    pub content_type: Option<String>,
    pub business_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferChunk {
    pub index: usize,
    pub range: ByteRange,
}

#[derive(Debug, Clone)]
pub struct ResumeCheckpoint {
    pub transfer_id: String,
    pub next_offset: u64,
    pub total_size: u64,
    pub etag: Option<String>,
    pub completed: Vec<ByteRange>,
}

pub trait ResumeStore: Send + Sync {
    fn load(&self, transfer_id: &str) -> Result<Option<ResumeCheckpoint>>;
    fn save(&self, checkpoint: ResumeCheckpoint) -> Result<()>;
    fn clear(&self, transfer_id: &str) -> Result<()>;
}

#[derive(Default)]
pub struct MemoryResumeStore {
    inner: Mutex<BTreeMap<String, ResumeCheckpoint>>,
}

impl ResumeStore for MemoryResumeStore {
    fn load(&self, transfer_id: &str) -> Result<Option<ResumeCheckpoint>> {
        let entries = self
            .inner
            .lock()
            .map_err(|_| NetError::Resume("resume store mutex poisoned".into()))?;
        Ok(entries.get(transfer_id).cloned())
    }

    fn save(&self, checkpoint: ResumeCheckpoint) -> Result<()> {
        let mut entries = self
            .inner
            .lock()
            .map_err(|_| NetError::Resume("resume store mutex poisoned".into()))?;
        entries.insert(checkpoint.transfer_id.clone(), checkpoint);
        Ok(())
    }

    fn clear(&self, transfer_id: &str) -> Result<()> {
        let mut entries = self
            .inner
            .lock()
            .map_err(|_| NetError::Resume("resume store mutex poisoned".into()))?;
        entries.remove(transfer_id);
        Ok(())
    }
}

pub struct ResumableTransferManager {
    policy: ResumePolicy,
}

impl ResumableTransferManager {
    pub fn new(policy: ResumePolicy) -> Self {
        Self { policy }
    }

    pub fn plan(
        &self,
        spec: &TransferSpec,
        checkpoint: Option<&ResumeCheckpoint>,
    ) -> Vec<TransferChunk> {
        let mut chunks = Vec::new();
        let mut offset = checkpoint.map(|point| point.next_offset).unwrap_or(0);
        let chunk_size = self.policy.chunk_size as u64;

        while offset < spec.total_size {
            let end = (offset + chunk_size).min(spec.total_size);
            chunks.push(TransferChunk {
                index: chunks.len(),
                range: ByteRange { start: offset, end },
            });
            offset = end;
        }

        chunks
    }

    pub fn checkpoint_after(
        &self,
        spec: &TransferSpec,
        chunk: &TransferChunk,
        previous: Option<&ResumeCheckpoint>,
    ) -> ResumeCheckpoint {
        let mut completed = previous
            .map(|checkpoint| checkpoint.completed.clone())
            .unwrap_or_default();
        completed.push(chunk.range.clone());
        ResumeCheckpoint {
            transfer_id: spec.transfer_id.clone(),
            next_offset: chunk.range.end,
            total_size: spec.total_size,
            etag: spec.etag.clone(),
            completed,
        }
    }

    pub fn build_segment_body(
        &self,
        chunk: &TransferChunk,
        bytes: Vec<u8>,
        total_size: u64,
    ) -> Body {
        Body::Segment {
            offset: chunk.range.start,
            total_size: Some(total_size),
            bytes,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub request: Request,
    pub spec: TransferSpec,
}

#[derive(Debug, Clone)]
pub struct DownloadOutcome {
    pub bytes: Vec<u8>,
    pub chunks: Vec<TransferChunk>,
    pub checkpoints: Vec<ResumeCheckpoint>,
    pub final_response: Response,
}

#[derive(Debug, Clone)]
pub struct UploadRequest {
    pub request: Request,
    pub spec: TransferSpec,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct UploadOutcome {
    pub uploaded_bytes: usize,
    pub chunks: Vec<TransferChunk>,
    pub checkpoints: Vec<ResumeCheckpoint>,
    pub final_response: Response,
}
