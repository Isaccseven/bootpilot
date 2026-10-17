//! Restricted Windows boot access.
pub mod bcd;
pub mod broker;
pub mod firmware;
#[cfg(windows)]
mod native;
pub mod snapshot;
pub mod store;
pub mod volumes;
pub use broker::{BrokerClient, BrokerRequest, BrokerResponse};
pub use snapshot::{WindowsSystem, collect_snapshot};
pub use store::ProtectedStore;

pub(crate) fn error(code: boot_core::ErrorCode, message: &str) -> boot_core::BootError {
    boot_core::BootError::new(code, message)
}
pub(crate) fn io_error(e: impl std::fmt::Display + 'static) -> boot_core::BootError {
    let code = match (&e as &dyn std::any::Any)
        .downcast_ref::<std::io::Error>()
        .map(std::io::Error::kind)
    {
        Some(std::io::ErrorKind::PermissionDenied) => boot_core::ErrorCode::AccessDenied,
        Some(std::io::ErrorKind::NotFound) => boot_core::ErrorCode::NotFound,
        _ => boot_core::ErrorCode::Io,
    };
    error(code, "Windows-Zugriff fehlgeschlagen")
        .with_details(serde_json::json!({"reason":e.to_string()}))
}
