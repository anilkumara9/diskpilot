use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("scan cancelled")]
    Cancelled,
    #[error("path rejected by safety gate: {0}")]
    SafetyRejected(String),
    #[error("plan not found: {0}")]
    PlanNotFound(String),
    #[error("plan expired: {0}")]
    PlanExpired(String),
    #[error("plan already used: {0}")]
    PlanAlreadyUsed(String),
    #[error("docker not available: {0}")]
    DockerUnavailable(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("{0}")]
    Message(String),
}

pub type CoreResult<T> = Result<T, CoreError>;
