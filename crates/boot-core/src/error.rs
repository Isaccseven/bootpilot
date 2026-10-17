use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

pub type BootResult<T> = Result<T, BootError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    UnknownEntry,
    IncompleteSnapshot,
    UnsupportedConfiguration,
    InvalidCandidate,
    InvalidTitle,
    InvalidBackup,
    InvalidSchema,
    AccessDenied,
    UacCancelled,
    Timeout,
    NotImplemented,
    Conflict,
    NotFound,
    Io,
    StorageUnsafe,
    InvalidMessage,
    UntrustedPeer,
    PlanExpired,
    PlanConsumed,
    VerificationFailed,
    BackupFailed,
    RollbackFailed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BootError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Value,
}

impl BootError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Value::Null,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }
}

impl fmt::Display for BootError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for BootError {}
