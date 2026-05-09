#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Empty,
    Bytes(Vec<u8>),
    Text(String),
    Segment {
        offset: u64,
        total_size: Option<u64>,
        bytes: Vec<u8>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    Empty,
    Binary,
    Text,
    Segment,
}

impl Body {
    pub fn kind(&self) -> BodyKind {
        match self {
            Body::Empty => BodyKind::Empty,
            Body::Bytes(_) => BodyKind::Binary,
            Body::Text(_) => BodyKind::Text,
            Body::Segment { .. } => BodyKind::Segment,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Body::Empty => 0,
            Body::Bytes(bytes) => bytes.len(),
            Body::Text(text) => text.len(),
            Body::Segment { bytes, .. } => bytes.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Body::Empty => Vec::new(),
            Body::Bytes(bytes) => bytes,
            Body::Text(text) => text.into_bytes(),
            Body::Segment { bytes, .. } => bytes,
        }
    }
}

impl Default for Body {
    fn default() -> Self {
        Self::Empty
    }
}

impl From<Vec<u8>> for Body {
    fn from(value: Vec<u8>) -> Self {
        Self::Bytes(value)
    }
}

impl From<&[u8]> for Body {
    fn from(value: &[u8]) -> Self {
        Self::Bytes(value.to_vec())
    }
}

impl From<String> for Body {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for Body {
    fn from(value: &str) -> Self {
        Self::Text(value.to_string())
    }
}
