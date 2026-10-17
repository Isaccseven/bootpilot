//! Shared, platform-independent boot configuration contracts.

pub mod diagnose;
pub mod error;
pub mod model;

pub use diagnose::{diagnose, validate_request};
pub use error::{BootError, BootResult, ErrorCode};
pub use model::*;
