//! Shared application service. Cached data never authorizes a boot operation.
mod client;
pub mod integration;

use boot_core::*;
use boot_windows::{BrokerClient, BrokerRequest, BrokerResponse};
use std::{
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};
use uuid::Uuid;

/// Blocking service boundary. GUI/MCP callers use a blocking worker for broker calls.
/// Reading status/diagnosis never starts the UAC helper.
pub struct BootService {
    client: BrokerClient,
    cache_path: Option<PathBuf>,
    snapshot: Mutex<Option<Snapshot>>,
    operation: Mutex<()>,
}

impl Default for BootService {
    fn default() -> Self {
        Self::new()
    }
}

impl BootService {
    pub fn new() -> Self {
        Self::with_client_and_cache(BrokerClient::new(), client::default_cache_path())
    }

    /// An explicit `None` cache is memory-only, suitable for injected transports.
    /// Production executables always use `new`; no environment flag enables fixtures.
    pub fn with_client_and_cache(client: BrokerClient, cache_path: Option<PathBuf>) -> Self {
        Self {
            client,
            cache_path,
            snapshot: Mutex::new(None),
            operation: Mutex::new(()),
        }
    }

    pub fn get_status(&self) -> BootResult<Option<Snapshot>> {
        if let Some(path) = &self.cache_path {
            return client::read_cache(path);
        }
        self.snapshot
            .lock()
            .map(|value| value.clone())
            .map_err(|_| poisoned())
    }

    pub fn refresh(&self) -> BootResult<Snapshot> {
        let _operation = self.lock_operation()?;
        let snapshot = match self.client.request(BrokerRequest::Refresh)? {
            BrokerResponse::Snapshot(snapshot) => *snapshot,
            _ => return Err(unexpected()),
        };
        client::validate_snapshot(&snapshot)?;
        // Only a successful collection can replace the cached capture. In particular,
        // a cancelled UAC prompt cannot change its timestamp or source completeness.
        if let Some(path) = &self.cache_path {
            client::write_cache(path, &snapshot)?;
        }
        *self.snapshot.lock().map_err(|_| poisoned())? = Some(snapshot.clone());
        Ok(snapshot)
    }

    pub fn diagnose(&self) -> BootResult<Vec<Finding>> {
        let snapshot = self.get_status()?.ok_or_else(|| {
            BootError::new(
                ErrorCode::IncompleteSnapshot,
                "Noch keine Erfassung. Zuerst ausdrücklich aktualisieren.",
            )
        })?;
        Ok(boot_core::diagnose(&snapshot))
    }

    pub fn prepare(&self, request: PlanRequest) -> BootResult<ChangePlan> {
        let _operation = self.lock_operation()?;
        // The helper collects fresh state and validates the request. A cached scan ID
        // is not a target checksum, nor an authorization or a stable system identity.
        match self.client.request(BrokerRequest::Prepare(request))? {
            BrokerResponse::ChangePlan(plan) => Ok(plan),
            _ => Err(unexpected()),
        }
    }

    pub fn apply(&self, plan_id: Uuid) -> BootResult<ApplyReport> {
        if plan_id.is_nil() {
            return Err(BootError::new(
                ErrorCode::InvalidRequest,
                "Eine gültige Plan-ID ist erforderlich.",
            ));
        }
        let _operation = self.lock_operation()?;
        match self.client.request(BrokerRequest::Apply { plan_id })? {
            BrokerResponse::ApplyReport(report) => Ok(report),
            _ => Err(unexpected()),
        }
    }

    pub fn list_backups(&self) -> BootResult<Vec<BackupRecord>> {
        let _operation = self.lock_operation()?;
        match self.client.request(BrokerRequest::ListBackups)? {
            BrokerResponse::Backups(backups) => Ok(backups),
            _ => Err(unexpected()),
        }
    }

    fn lock_operation(&self) -> BootResult<MutexGuard<'_, ()>> {
        self.operation.lock().map_err(|_| poisoned())
    }
}

fn poisoned() -> BootError {
    BootError::new(ErrorCode::Io, "Der Dienstzustand ist nicht verfügbar.")
}
fn unexpected() -> BootError {
    BootError::new(
        ErrorCode::InvalidMessage,
        "Der Helfer hat einen unerwarteten Antworttyp geliefert.",
    )
}
