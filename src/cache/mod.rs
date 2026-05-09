use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::config::{BusinessContext, CachePolicy};
use crate::error::{NetError, Result};
use crate::types::{Body, Request, Response};

#[derive(Debug, Clone)]
struct CacheEntry {
    response: Response,
    inserted_at: Instant,
    expires_at: Instant,
    stale_until: Instant,
}

#[derive(Debug, Clone)]
pub enum CacheLookup {
    Hit(Response),
    Stale(Response),
    Miss,
}

#[derive(Debug, Clone, Default)]
pub struct CacheSummary {
    pub key: Option<String>,
    pub hit: bool,
    pub stale: bool,
}

pub trait CacheStore: Send + Sync {
    fn get(&self, key: &str) -> Result<CacheLookup>;
    fn put(
        &self,
        key: String,
        response: Response,
        ttl: Duration,
        stale_if_error: Duration,
    ) -> Result<()>;
    fn invalidate(&self, key: &str) -> Result<()>;
}

pub struct MemoryCache {
    capacity: usize,
    inner: Mutex<BTreeMap<String, CacheEntry>>,
}

impl MemoryCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            inner: Mutex::new(BTreeMap::new()),
        }
    }

    fn evict_if_needed(&self, entries: &mut BTreeMap<String, CacheEntry>) {
        while entries.len() >= self.capacity.max(1) {
            let oldest = entries
                .iter()
                .min_by_key(|(_, entry)| entry.inserted_at)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                entries.remove(&oldest);
            } else {
                break;
            }
        }
    }
}

pub struct DiskCache {
    root: PathBuf,
}

impl DiskCache {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)
            .map_err(|error| NetError::Cache(format!("failed to create cache dir: {error}")))?;
        Ok(Self { root })
    }

    fn path_for(&self, key: &str) -> PathBuf {
        let digest = hash_key(key);
        self.root.join(format!("{digest}.cache"))
    }

    fn read_entry(&self, path: &Path) -> Result<Option<DiskCacheEntry>> {
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(path)
            .map_err(|error| NetError::Cache(format!("failed to read cache file: {error}")))?;
        Ok(Some(DiskCacheEntry::decode(&bytes)?))
    }
}

impl Default for MemoryCache {
    fn default() -> Self {
        Self::new(1_024)
    }
}

impl CacheStore for MemoryCache {
    fn get(&self, key: &str) -> Result<CacheLookup> {
        let now = Instant::now();
        let mut entries = self
            .inner
            .lock()
            .map_err(|_| NetError::Cache("cache mutex poisoned".into()))?;
        match entries.get(key) {
            Some(entry) if now <= entry.expires_at => Ok(CacheLookup::Hit(entry.response.clone())),
            Some(entry) if now <= entry.stale_until => {
                Ok(CacheLookup::Stale(entry.response.clone()))
            }
            Some(_) => {
                entries.remove(key);
                Ok(CacheLookup::Miss)
            }
            None => Ok(CacheLookup::Miss),
        }
    }

    fn put(
        &self,
        key: String,
        response: Response,
        ttl: Duration,
        stale_if_error: Duration,
    ) -> Result<()> {
        let now = Instant::now();
        let mut entries = self
            .inner
            .lock()
            .map_err(|_| NetError::Cache("cache mutex poisoned".into()))?;
        self.evict_if_needed(&mut entries);
        entries.insert(
            key,
            CacheEntry {
                response,
                inserted_at: now,
                expires_at: now + ttl,
                stale_until: now + ttl + stale_if_error,
            },
        );
        Ok(())
    }

    fn invalidate(&self, key: &str) -> Result<()> {
        let mut entries = self
            .inner
            .lock()
            .map_err(|_| NetError::Cache("cache mutex poisoned".into()))?;
        entries.remove(key);
        Ok(())
    }
}

impl CacheStore for DiskCache {
    fn get(&self, key: &str) -> Result<CacheLookup> {
        let path = self.path_for(key);
        let Some(entry) = self.read_entry(&path)? else {
            return Ok(CacheLookup::Miss);
        };
        let now = now_millis();
        if now <= entry.expires_at_ms {
            return Ok(CacheLookup::Hit(entry.response));
        }
        if now <= entry.stale_until_ms {
            return Ok(CacheLookup::Stale(entry.response));
        }
        let _ = fs::remove_file(path);
        Ok(CacheLookup::Miss)
    }

    fn put(
        &self,
        key: String,
        response: Response,
        ttl: Duration,
        stale_if_error: Duration,
    ) -> Result<()> {
        let now = now_millis();
        let entry = DiskCacheEntry {
            response,
            expires_at_ms: now + ttl.as_millis() as u64,
            stale_until_ms: now + (ttl + stale_if_error).as_millis() as u64,
        };
        let path = self.path_for(&key);
        let temp = path.with_extension("tmp");
        fs::write(&temp, entry.encode()?)
            .map_err(|error| NetError::Cache(format!("failed to write cache file: {error}")))?;
        fs::rename(&temp, &path)
            .map_err(|error| NetError::Cache(format!("failed to finalize cache file: {error}")))?;
        Ok(())
    }

    fn invalidate(&self, key: &str) -> Result<()> {
        let path = self.path_for(key);
        if path.exists() {
            fs::remove_file(&path).map_err(|error| {
                NetError::Cache(format!(
                    "failed to remove cache file `{}`: {error}",
                    path.display()
                ))
            })?;
        }
        Ok(())
    }
}

pub fn build_cache_key(
    request: &Request,
    business: &BusinessContext,
    policy: &CachePolicy,
) -> String {
    let namespace = request
        .business_profile
        .as_ref()
        .and_then(|profile| profile.cache_namespace.clone())
        .unwrap_or_else(|| business.app.clone());
    let vary = policy
        .vary_headers
        .iter()
        .map(|header| {
            let value = request.headers.get(header).unwrap_or_default();
            format!("{header}={value}")
        })
        .collect::<Vec<_>>()
        .join("&");
    format!(
        "{namespace}|{:?}|{}|{}|{vary}",
        request.method,
        request.endpoint.origin(),
        request.endpoint.uri()
    )
}

#[derive(Debug, Clone)]
struct DiskCacheEntry {
    response: Response,
    expires_at_ms: u64,
    stale_until_ms: u64,
}

impl DiskCacheEntry {
    fn encode(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"ATLAS-CACHE-V1");
        write_u64(&mut bytes, self.expires_at_ms)?;
        write_u64(&mut bytes, self.stale_until_ms)?;
        write_u16(&mut bytes, self.response.status)?;
        let (body_kind, offset, total_size, body_bytes) = encode_body(&self.response.body);
        write_u8(&mut bytes, body_kind)?;
        write_u64(&mut bytes, offset)?;
        write_u64(&mut bytes, total_size.unwrap_or(u64::MAX))?;
        write_u32(&mut bytes, self.response.headers.iter().count() as u32)?;
        for (key, value) in self.response.headers.iter() {
            write_string(&mut bytes, key)?;
            write_string(&mut bytes, value)?;
        }
        write_bytes(&mut bytes, &body_bytes)?;
        Ok(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(bytes);
        let mut magic = [0u8; 14];
        cursor
            .read_exact(&mut magic)
            .map_err(|error| NetError::Cache(format!("invalid cache magic: {error}")))?;
        if &magic != b"ATLAS-CACHE-V1" {
            return Err(NetError::Cache("unrecognized disk cache version".into()));
        }

        let expires_at_ms = read_u64(&mut cursor)?;
        let stale_until_ms = read_u64(&mut cursor)?;
        let status = read_u16(&mut cursor)?;
        let body_kind = read_u8(&mut cursor)?;
        let offset = read_u64(&mut cursor)?;
        let total_size_raw = read_u64(&mut cursor)?;
        let header_count = read_u32(&mut cursor)?;
        let mut response = Response::new(status);
        for _ in 0..header_count {
            let key = read_string(&mut cursor)?;
            let value = read_string(&mut cursor)?;
            response.headers.insert(key, value);
        }
        let body_bytes = read_bytes(&mut cursor)?;
        response.body = decode_body(body_kind, offset, total_size_raw, body_bytes)?;

        Ok(Self {
            response,
            expires_at_ms,
            stale_until_ms,
        })
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn hash_key(key: &str) -> String {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn encode_body(body: &Body) -> (u8, u64, Option<u64>, Vec<u8>) {
    match body {
        Body::Empty => (0, 0, None, Vec::new()),
        Body::Bytes(bytes) => (1, 0, None, bytes.clone()),
        Body::Text(text) => (2, 0, None, text.as_bytes().to_vec()),
        Body::Segment {
            offset,
            total_size,
            bytes,
        } => (3, *offset, *total_size, bytes.clone()),
    }
}

fn decode_body(body_kind: u8, offset: u64, total_size_raw: u64, bytes: Vec<u8>) -> Result<Body> {
    match body_kind {
        0 => Ok(Body::Empty),
        1 => Ok(Body::Bytes(bytes)),
        2 => String::from_utf8(bytes)
            .map(Body::Text)
            .map_err(|error| NetError::Cache(format!("invalid text body in cache: {error}"))),
        3 => Ok(Body::Segment {
            offset,
            total_size: if total_size_raw == u64::MAX {
                None
            } else {
                Some(total_size_raw)
            },
            bytes,
        }),
        _ => Err(NetError::Cache(format!(
            "unsupported cached body kind `{body_kind}`"
        ))),
    }
}

fn write_u8(bytes: &mut Vec<u8>, value: u8) -> Result<()> {
    bytes.write_all(&[value]).map_err(io_error)
}

fn write_u16(bytes: &mut Vec<u8>, value: u16) -> Result<()> {
    bytes.write_all(&value.to_le_bytes()).map_err(io_error)
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) -> Result<()> {
    bytes.write_all(&value.to_le_bytes()).map_err(io_error)
}

fn write_u64(bytes: &mut Vec<u8>, value: u64) -> Result<()> {
    bytes.write_all(&value.to_le_bytes()).map_err(io_error)
}

fn write_string(bytes: &mut Vec<u8>, value: &str) -> Result<()> {
    write_bytes(bytes, value.as_bytes())
}

fn write_bytes(bytes: &mut Vec<u8>, value: &[u8]) -> Result<()> {
    write_u32(bytes, value.len() as u32)?;
    bytes.write_all(value).map_err(io_error)
}

fn read_u8(cursor: &mut Cursor<&[u8]>) -> Result<u8> {
    let mut buf = [0u8; 1];
    cursor.read_exact(&mut buf).map_err(io_error)?;
    Ok(buf[0])
}

fn read_u16(cursor: &mut Cursor<&[u8]>) -> Result<u16> {
    let mut buf = [0u8; 2];
    cursor.read_exact(&mut buf).map_err(io_error)?;
    Ok(u16::from_le_bytes(buf))
}

fn read_u32(cursor: &mut Cursor<&[u8]>) -> Result<u32> {
    let mut buf = [0u8; 4];
    cursor.read_exact(&mut buf).map_err(io_error)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64(cursor: &mut Cursor<&[u8]>) -> Result<u64> {
    let mut buf = [0u8; 8];
    cursor.read_exact(&mut buf).map_err(io_error)?;
    Ok(u64::from_le_bytes(buf))
}

fn read_string(cursor: &mut Cursor<&[u8]>) -> Result<String> {
    let bytes = read_bytes(cursor)?;
    String::from_utf8(bytes)
        .map_err(|error| NetError::Cache(format!("invalid string in cache: {error}")))
}

fn read_bytes(cursor: &mut Cursor<&[u8]>) -> Result<Vec<u8>> {
    let len = read_u32(cursor)? as usize;
    let mut bytes = vec![0u8; len];
    cursor.read_exact(&mut bytes).map_err(io_error)?;
    Ok(bytes)
}

fn io_error(error: std::io::Error) -> NetError {
    NetError::Cache(error.to_string())
}
