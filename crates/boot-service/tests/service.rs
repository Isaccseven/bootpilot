use boot_core::*;
use boot_service::BootService;
use boot_windows::{BrokerClient, BrokerRequest, BrokerResponse, broker::BrokerTransport};
use chrono::{TimeZone, Utc};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

struct Transport {
    replies: Mutex<VecDeque<BootResult<BrokerResponse>>>,
    requests: Arc<Mutex<Vec<BrokerRequest>>>,
}
impl BrokerTransport for Transport {
    fn exchange(&self, request: BrokerRequest) -> BootResult<BrokerResponse> {
        self.requests.lock().unwrap().push(request);
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected broker call")
    }
}
fn service(
    replies: Vec<BootResult<BrokerResponse>>,
) -> (BootService, Arc<Mutex<Vec<BrokerRequest>>>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let client = BrokerClient::with_transport(Transport {
        replies: Mutex::new(replies.into()),
        requests: requests.clone(),
    });
    (BootService::with_client_and_cache(client, None), requests)
}
fn snapshot() -> Snapshot {
    Snapshot::new(
        Uuid::new_v4(),
        Utc.with_ymd_and_hms(2026, 10, 1, 12, 0, 0).unwrap(),
    )
}
#[test]
fn status_and_diagnose_do_not_elevate_or_invent_data() {
    let (service, requests) = service(vec![]);
    assert_eq!(service.get_status().unwrap(), None);
    assert_eq!(
        service.diagnose().unwrap_err().code,
        ErrorCode::IncompleteSnapshot
    );
    assert!(requests.lock().unwrap().is_empty());
}
#[test]
fn failed_refresh_keeps_capture_time_and_completeness() {
    let original = snapshot();
    let (service, _) = service(vec![
        Ok(BrokerResponse::Snapshot(Box::new(original.clone()))),
        Err(BootError::new(ErrorCode::UacCancelled, "UAC abgebrochen")),
    ]);
    assert_eq!(service.refresh().unwrap(), original);
    assert_eq!(service.refresh().unwrap_err().code, ErrorCode::UacCancelled);
    assert_eq!(service.get_status().unwrap(), Some(original));
}
#[test]
fn prepare_delegates_to_helper_without_cached_snapshot_identity_gate() {
    let (service, requests) = service(vec![Err(BootError::new(
        ErrorCode::InvalidBackup,
        "Sicherung fehlt",
    ))]);
    let backup_id = Uuid::new_v4();
    assert_eq!(
        service
            .prepare(PlanRequest::Restore { backup_id })
            .unwrap_err()
            .code,
        ErrorCode::InvalidBackup
    );
    assert!(
        matches!(requests.lock().unwrap().as_slice(), [BrokerRequest::Prepare(PlanRequest::Restore { backup_id: id })] if *id == backup_id)
    );
}
#[test]
fn wrong_response_does_not_poison_status_cache() {
    let (service, _) = service(vec![Ok(BrokerResponse::Backups(vec![]))]);
    assert_eq!(
        service.refresh().unwrap_err().code,
        ErrorCode::InvalidMessage
    );
    assert_eq!(service.get_status().unwrap(), None);
}
#[test]
fn successful_capture_survives_service_restart() {
    let directory = std::env::temp_dir().join(format!("bootpilot-service-test-{}", Uuid::new_v4()));
    let cache = directory.join("snapshot.json");
    let original = snapshot();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let client = BrokerClient::with_transport(Transport {
        replies: Mutex::new(vec![Ok(BrokerResponse::Snapshot(Box::new(original.clone())))].into()),
        requests,
    });
    BootService::with_client_and_cache(client, Some(cache.clone()))
        .refresh()
        .unwrap();
    let restarted = BootService::with_client_and_cache(BrokerClient::new(), Some(cache));
    assert_eq!(restarted.get_status().unwrap(), Some(original));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn helper_plan_scan_id_can_differ_from_cached_scan_id() {
    let original = snapshot();
    let plan = ChangePlan {
        schema_version: SCHEMA_VERSION,
        id: Uuid::new_v4(),
        created_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::minutes(15),
        snapshot_id: Uuid::new_v4(),
        request: PlanRequest::Restore {
            backup_id: Uuid::new_v4(),
        },
        diffs: vec![],
        preconditions: vec![],
        changes: vec![],
        state: PlanState::Ready,
    };
    assert_ne!(plan.snapshot_id, original.id);
    let (service, _) = service(vec![
        Ok(BrokerResponse::Snapshot(Box::new(original))),
        Ok(BrokerResponse::ChangePlan(plan.clone())),
    ]);
    service.refresh().unwrap();
    assert_eq!(service.prepare(plan.request.clone()).unwrap(), plan);
}

#[test]
fn persistent_refresh_replaces_cache_and_cancel_preserves_it() {
    let directory = std::env::temp_dir().join(format!("bootpilot-service-test-{}", Uuid::new_v4()));
    let cache = directory.join("snapshot.json");
    let first = snapshot();
    let mut second = snapshot();
    second.sources.firmware = CollectionState::Partial;
    let client = BrokerClient::with_transport(Transport {
        replies: Mutex::new(
            vec![
                Ok(BrokerResponse::Snapshot(Box::new(first))),
                Ok(BrokerResponse::Snapshot(Box::new(second.clone()))),
                Err(BootError::new(ErrorCode::UacCancelled, "UAC abgebrochen")),
            ]
            .into(),
        ),
        requests: Arc::new(Mutex::new(vec![])),
    });
    let service = BootService::with_client_and_cache(client, Some(cache));
    service.refresh().unwrap();
    service.refresh().unwrap();
    let before = service.get_status().unwrap();
    assert_eq!(before, Some(second));
    assert!(service.refresh().is_err());
    assert_eq!(service.get_status().unwrap(), before);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn apply_and_backups_forward_exact_broker_response() {
    let plan_id = Uuid::new_v4();
    let report = ApplyReport {
        schema_version: SCHEMA_VERSION,
        plan_id,
        backup_id: None,
        outcome: ApplyOutcome::Partial,
        verified_steps: vec![],
        remaining_deviations: vec![],
        boot_tested: false,
    };
    let (service, requests) = service(vec![
        Ok(BrokerResponse::ApplyReport(report.clone())),
        Ok(BrokerResponse::Backups(vec![])),
    ]);
    assert_eq!(
        service.apply(Uuid::nil()).unwrap_err().code,
        ErrorCode::InvalidRequest
    );
    assert_eq!(service.apply(plan_id).unwrap(), report);
    assert!(service.list_backups().unwrap().is_empty());
    assert!(
        matches!(requests.lock().unwrap().as_slice(), [BrokerRequest::Apply { plan_id: id }, BrokerRequest::ListBackups] if *id == plan_id)
    );
}

#[test]
fn corrupt_cache_is_an_error_and_never_sample_data() {
    let directory = std::env::temp_dir().join(format!("bootpilot-service-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let cache = directory.join("snapshot.json");
    std::fs::write(&cache, b"{ broken").unwrap();
    let service = BootService::with_client_and_cache(BrokerClient::new(), Some(cache));
    assert_eq!(
        service.get_status().unwrap_err().code,
        ErrorCode::InvalidSchema
    );
    std::fs::remove_dir_all(directory).unwrap();
}
