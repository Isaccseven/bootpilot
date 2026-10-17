use boot_core::*;
use boot_service::BootService;
use boot_windows::{BrokerClient, BrokerRequest, BrokerResponse, broker::BrokerTransport};
use chrono::Utc;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

struct Responses {
    replies: Mutex<VecDeque<BootResult<BrokerResponse>>>,
    requests: Arc<Mutex<Vec<BrokerRequest>>>,
}
impl BrokerTransport for Responses {
    fn exchange(&self, request: BrokerRequest) -> BootResult<BrokerResponse> {
        self.requests.lock().unwrap().push(request);
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected broker call")
    }
}
fn setup(
    cache: Option<std::path::PathBuf>,
    reply: BootResult<BrokerResponse>,
) -> (BootService, Arc<Mutex<Vec<BrokerRequest>>>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let client = BrokerClient::with_transport(Responses {
        replies: Mutex::new(
            vec![
                Ok(BrokerResponse::Snapshot(Box::new(Snapshot::new(
                    Uuid::new_v4(),
                    Utc::now(),
                )))),
                reply,
            ]
            .into(),
        ),
        requests: requests.clone(),
    });
    (BootService::with_client_and_cache(client, cache), requests)
}
fn applied(outcome: ApplyOutcome) -> BootResult<BrokerResponse> {
    Ok(BrokerResponse::ApplyReport(ApplyReport {
        schema_version: SCHEMA_VERSION,
        plan_id: Uuid::new_v4(),
        backup_id: None,
        outcome,
        verified_steps: vec![],
        remaining_deviations: vec![],
        boot_tested: false,
    }))
}
#[test]
fn every_apply_outcome_invalidates_persistent_cache_for_other_clients_and_restart() {
    let replies = vec![
        applied(ApplyOutcome::Applied),
        applied(ApplyOutcome::Partial),
        applied(ApplyOutcome::RolledBack),
        Err(BootError::new(
            ErrorCode::Io,
            "Verbindung nach möglichem Schreibschritt abgebrochen",
        )),
        Ok(BrokerResponse::Backups(vec![])),
    ];
    for reply in replies {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("snapshot.json");
        let (service, _) = setup(Some(cache.clone()), reply);
        let captured = service.refresh().unwrap();
        let already_running =
            BootService::with_client_and_cache(BrokerClient::new(), Some(cache.clone()));
        assert_eq!(
            already_running.get_status().unwrap(),
            Some(captured.clone())
        );
        let _ = service.apply(Uuid::new_v4());
        assert_eq!(
            service.get_status().unwrap(),
            None,
            "applying client must not serve pre-apply capture"
        );
        assert_eq!(
            already_running.get_status().unwrap(),
            None,
            "existing second client must see invalidation"
        );
        let restarted = BootService::with_client_and_cache(BrokerClient::new(), Some(cache));
        assert_eq!(
            restarted.get_status().unwrap(),
            None,
            "invalidation must survive process restart"
        );
    }
}
#[test]
fn uncertain_apply_invalidates_memory_cache_too() {
    let (service, _) = setup(
        None,
        Err(BootError::new(ErrorCode::Timeout, "Unbekanntes Ergebnis")),
    );
    service.refresh().unwrap();
    assert!(service.get_status().unwrap().is_some());
    assert!(service.apply(Uuid::new_v4()).is_err());
    assert_eq!(service.get_status().unwrap(), None);
}
#[test]
fn cache_invalidation_failure_prevents_mutation_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("snapshot.json");
    let (service, requests) = setup(Some(cache.clone()), applied(ApplyOutcome::Applied));
    service.refresh().unwrap();
    std::fs::remove_file(&cache).unwrap();
    std::fs::create_dir(&cache).unwrap();
    assert!(service.apply(Uuid::new_v4()).is_err());
    assert!(matches!(
        requests.lock().unwrap().as_slice(),
        [BrokerRequest::Refresh]
    ));
}

struct DelayedRefresh {
    entered: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}
impl BrokerTransport for DelayedRefresh {
    fn exchange(&self, request: BrokerRequest) -> BootResult<BrokerResponse> {
        assert!(matches!(request, BrokerRequest::Refresh));
        self.entered.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        Ok(BrokerResponse::Snapshot(Box::new(Snapshot::new(
            Uuid::new_v4(),
            Utc::now(),
        ))))
    }
}
struct SignalApply(std::sync::mpsc::Sender<()>);
impl BrokerTransport for SignalApply {
    fn exchange(&self, request: BrokerRequest) -> BootResult<BrokerResponse> {
        assert!(matches!(request, BrokerRequest::Apply { .. }));
        self.0.send(()).unwrap();
        applied(ApplyOutcome::Applied)
    }
}
#[test]
fn concurrent_older_refresh_cannot_repopulate_cache_after_apply() {
    use std::{sync::mpsc, time::Duration};
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path().join("snapshot.json");
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let refresh_service = BootService::with_client_and_cache(
        BrokerClient::with_transport(DelayedRefresh {
            entered: entered_tx,
            release: Mutex::new(release_rx),
        }),
        Some(cache.clone()),
    );
    let refresh = std::thread::spawn(move || refresh_service.refresh());
    entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let (called_tx, called_rx) = mpsc::channel();
    let apply_service = BootService::with_client_and_cache(
        BrokerClient::with_transport(SignalApply(called_tx)),
        Some(cache.clone()),
    );
    let apply = std::thread::spawn(move || apply_service.apply(Uuid::new_v4()));
    let dispatched_before_refresh_finished =
        called_rx.recv_timeout(Duration::from_millis(200)).is_ok();
    release_tx.send(()).unwrap();
    refresh.join().unwrap().unwrap();
    apply.join().unwrap().unwrap();
    assert!(
        !dispatched_before_refresh_finished,
        "cross-client apply must wait for the in-flight refresh lease"
    );
    let reader = BootService::with_client_and_cache(BrokerClient::new(), Some(cache));
    assert_eq!(reader.get_status().unwrap(), None);
}
