use std::collections::BTreeMap;

pub type HeaderMap = BTreeMap<String, String>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Headers {
    inner: HeaderMap,
}

impl Headers {
    pub fn new() -> Self {
        Self {
            inner: HeaderMap::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.inner
            .insert(Self::normalize_key(key.into()), value.into());
    }

    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.insert(key, value);
        self
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.inner
            .get(&Self::normalize_key(key.to_string()))
            .map(String::as_str)
    }

    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.inner.remove(&Self::normalize_key(key.to_string()))
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.inner
            .contains_key(&Self::normalize_key(key.to_string()))
    }

    pub fn extend(&mut self, other: &Headers) {
        for (key, value) in &other.inner {
            self.inner.insert(key.clone(), value.clone());
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.inner.iter()
    }

    pub fn into_inner(self) -> HeaderMap {
        self.inner
    }

    fn normalize_key(key: String) -> String {
        key.trim().to_ascii_lowercase()
    }
}

impl From<HeaderMap> for Headers {
    fn from(value: HeaderMap) -> Self {
        let mut headers = Self::new();
        for (key, value) in value {
            headers.insert(key, value);
        }
        headers
    }
}
