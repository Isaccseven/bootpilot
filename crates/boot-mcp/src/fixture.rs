//! Explicitly test-only server; never included in production packaging.
use boot_core::*;
use boot_service::BootService;
use boot_windows::{BrokerClient, BrokerRequest, BrokerResponse, broker::BrokerTransport};
use chrono::{TimeZone, Utc};
use uuid::Uuid;

struct Fixture;
impl BrokerTransport for Fixture {
    fn exchange(&self, request: BrokerRequest) -> BootResult<BrokerResponse> {
        match request {
            BrokerRequest::Refresh => {
                let mut snapshot = Snapshot::new(
                    Uuid::from_u128(42),
                    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                );
                snapshot.sources.firmware = CollectionState::Unavailable;
                snapshot.source_errors.push(SourceError {
                    source: Source::Firmware,
                    error: BootError::new(
                        ErrorCode::AccessDenied,
                        "Testinstanz: Firmware nicht erfasst.",
                    ),
                });
                Ok(BrokerResponse::Snapshot(Box::new(snapshot)))
            }
            BrokerRequest::Prepare(PlanRequest::Restore { .. }) => Err(BootError::new(
                ErrorCode::InvalidBackup,
                "Testinstanz: Sicherung nicht vorhanden.",
            )),
            BrokerRequest::Prepare(_) => Err(BootError::new(
                ErrorCode::IncompleteSnapshot,
                "Testinstanz: unvollständige Erfassung.",
            )),
            BrokerRequest::Apply { .. } => Err(BootError::new(
                ErrorCode::AccessDenied,
                "Testinstanz: Schreiben gesperrt.",
            )),
            BrokerRequest::ListBackups => Ok(BrokerResponse::Backups(Vec::new())),
        }
    }
}
#[tokio::main]
async fn main() {
    eprintln!("BootPilot ausdrücklich gekennzeichnete MCP-Testinstanz; kein Windows-Zugriff.");
    if let Err(error) = boot_mcp::serve(BootService::with_client_and_cache(
        BrokerClient::with_transport(Fixture),
        None,
    ))
    .await
    {
        eprintln!("MCP-Testinstanz: {error}");
        std::process::exit(1);
    }
}
