use std::error::Error;
use std::fmt::{Display, Formatter};

pub type Result<T> = std::result::Result<T, NetError>;

#[derive(Debug, Clone)]
pub enum NetError {
    InvalidRequest(String),
    InvalidConfig(String),
    Auth(String),
    Cache(String),
    Dns(String),
    Cdn(String),
    Resume(String),
    Transport(String),
    CircuitOpen(String),
    RetryExhausted {
        attempts: usize,
        last_error: Box<NetError>,
    },
    PolicyViolation(String),
}

impl NetError {
    pub fn transport(message: impl Into<String>) -> Self {
        Self::Transport(message.into())
    }

    pub fn retry_exhausted(attempts: usize, last_error: NetError) -> Self {
        Self::RetryExhausted {
            attempts,
            last_error: Box::new(last_error),
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            NetError::Dns(_)
                | NetError::Transport(_)
                | NetError::Cache(_)
                | NetError::RetryExhausted { .. }
        )
    }
}

impl Display for NetError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            NetError::InvalidRequest(message) => write!(f, "invalid request: {message}"),
            NetError::InvalidConfig(message) => write!(f, "invalid config: {message}"),
            NetError::Auth(message) => write!(f, "auth error: {message}"),
            NetError::Cache(message) => write!(f, "cache error: {message}"),
            NetError::Dns(message) => write!(f, "dns error: {message}"),
            NetError::Cdn(message) => write!(f, "cdn error: {message}"),
            NetError::Resume(message) => write!(f, "resume error: {message}"),
            NetError::Transport(message) => write!(f, "transport error: {message}"),
            NetError::CircuitOpen(message) => write!(f, "circuit is open: {message}"),
            NetError::RetryExhausted {
                attempts,
                last_error,
            } => {
                write!(f, "retry exhausted after {attempts} attempts: {last_error}")
            }
            NetError::PolicyViolation(message) => write!(f, "policy violation: {message}"),
        }
    }
}

impl Error for NetError {}
