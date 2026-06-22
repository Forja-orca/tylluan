use thiserror::Error;

/// Unified error type for the TylluanNexus kernel.
#[derive(Error, Debug)]
pub enum TylluanError {
    #[error("Tool '{0}' not found in registry")]
    ToolNotFound(String),

    #[error("Guild '{0}' not found")]
    GuildNotFound(String),

    #[error("Guild '{0}' failed to load: {1}")]
    GuildLoadFailed(String, String),

    #[error("Security: {0}")]
    SecurityBlocked(String),

    #[error("Rate limit exceeded for session '{0}': max {1} calls/min")]
    RateLimitExceeded(String, u32),

    #[error("Circuit breaker OPEN for session '{0}': {1}")]
    CircuitBreakerOpen(String, String),

    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Memory error: {0}")]
    Memory(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type TylluanResult<T> = Result<T, TylluanError>;
