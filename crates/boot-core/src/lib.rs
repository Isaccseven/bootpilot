//! Shared, platform-independent boot configuration contracts.

pub mod diagnose;
pub mod error;
pub mod model;

pub use diagnose::{diagnose, validate_request};
pub use error::{BootError, BootResult, ErrorCode};
pub use model::*;
pub mod grub;
pub mod plan;
pub use grub::parse_grub;
pub use plan::{prepare_plan, target_checksum};
pub mod transaction;
pub use transaction::{apply_plan, prepare_restore};
