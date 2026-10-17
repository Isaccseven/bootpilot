use boot_core::*;
use chrono::{Duration, Utc};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use uuid::Uuid;

fn err(code: ErrorCode) -> BootError {
    BootError::new(code, "injected")
}
struct FakeSystem {
    snapshot: Snapshot,
    values: BTreeMap<Target, TargetValue>,
    writes: usize,
    fail: Option<usize>,
    race: bool,
    foreign: bool,
    concurrency: Option<Arc<AtomicUsize>>,
}
impl SystemAccess for FakeSystem {
    fn read_snapshot(&mut self) -> BootResult<Snapshot> {
        if let Some(active) = &self.concurrency {
            assert_eq!(active.fetch_add(1, Ordering::SeqCst), 0);
            std::thread::sleep(std::time::Duration::from_millis(15));
            active.fetch_sub(1, Ordering::SeqCst);
        }
        let mut s = self.snapshot.clone();
        s.id = Uuid::new_v4();
        if let (TargetValue::FileBytes(main), TargetValue::FileBytes(user)) = (
            &self.values[&Target::GrubMain],
            &self.values[&Target::GrubUser],
        ) {
            s.grub = Some(parse_grub(main, user)?);
        }
        Ok(s)
    }
    fn read_target(&mut self, target: &Target) -> BootResult<TargetValue> {
        Ok(self.values[target].clone())
    }
    fn write_target(
        &mut self,
        target: &Target,
        expected: &TargetValue,
        replacement: &TargetValue,
    ) -> BootResult<()> {
        self.writes += 1;
        if self.race && self.writes == 1 {
            self.values
                .insert(*target, TargetValue::FileBytes(b"foreign".to_vec()));
        }
        if self.fail == Some(self.writes) {
            if self.foreign {
                self.values.insert(
                    Target::GrubMain,
                    TargetValue::FileBytes(b"foreign".to_vec()),
                );
            }
            return Err(err(ErrorCode::Io));
        }
        if &self.values[target] != expected {
            return Err(err(ErrorCode::Conflict));
        }
        self.values.insert(*target, replacement.clone());
        Ok(())
    }
}
struct Store {
    plan: ChangePlan,
    backup: Option<BackupRecord>,
    fail_backup: bool,
    fail_step: Option<u32>,
    fail_finish: bool,
    directory: std::path::PathBuf,
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
impl Store {
    fn persist(&self) {
        std::fs::write(
            self.directory.join("journal.json"),
            serde_json::to_vec(&self.backup).unwrap(),
        )
        .unwrap();
    }
}
impl PlanStore for Store {
    fn save_plan(&mut self, p: &ChangePlan) -> BootResult<()> {
        self.plan = p.clone();
        Ok(())
    }
    fn load_plan(&self, _: Uuid) -> BootResult<ChangePlan> {
        Ok(self.plan.clone())
    }
    fn begin_backup(
        &mut self,
        p: &ChangePlan,
        originals: &[(Target, TargetValue)],
    ) -> BootResult<BackupRecord> {
        if self.fail_backup {
            return Err(err(ErrorCode::BackupFailed));
        }
        if self.plan.state != PlanState::Ready {
            return Err(err(ErrorCode::PlanConsumed));
        }
        self.plan.state = PlanState::Started;
        let b = BackupRecord {
            schema_version: 1,
            id: p.id,
            plan_id: p.id,
            created_at: Utc::now(),
            request: p.request.clone(),
            targets: p
                .changes
                .iter()
                .map(|c| {
                    assert!(originals.contains(&(c.target, c.expected.clone())));
                    BackupTarget {
                        target: c.target,
                        original: c.expected.clone(),
                        applied: c.replacement.clone(),
                        original_checksum: target_checksum(&c.expected),
                        applied_checksum: target_checksum(&c.replacement),
                    }
                })
                .collect(),
            journal: vec![],
            outcome: None,
        };
        self.backup = Some(b.clone());
        self.persist();
        Ok(b)
    }
    fn record_step(&mut self, _: Uuid, step: &JournalStep) -> BootResult<()> {
        if self.fail_step == Some(step.sequence) {
            return Err(err(ErrorCode::Io));
        }
        self.backup.as_mut().unwrap().journal.push(step.clone());
        self.persist();
        Ok(())
    }
    fn finish_plan(&mut self, _: Uuid, r: &ApplyReport) -> BootResult<()> {
        if self.fail_finish {
            return Err(err(ErrorCode::Io));
        }
        self.backup.as_mut().unwrap().outcome = Some(r.clone());
        self.plan.state = PlanState::Finished;
        self.persist();
        Ok(())
    }
    fn list_backups(&self) -> BootResult<Vec<BackupRecord>> {
        Ok(self.backup.clone().into_iter().collect())
    }
    fn load_backup(&self, _: Uuid) -> BootResult<BackupRecord> {
        self.backup.clone().ok_or(err(ErrorCode::NotFound))
    }
}
fn fixture() -> (FakeSystem, Store) {
    let mut s = Snapshot::new(Uuid::new_v4(), Utc::now());
    s.sources.grub = CollectionState::Complete;
    s.grub = Some(
        parse_grub(
            include_bytes!("../../../tests/fixtures/grub-main.cfg"),
            include_bytes!("../../../tests/fixtures/grub-user.cfg"),
        )
        .unwrap(),
    );
    let plan = prepare_plan(
        &s,
        PlanRequest::Menu {
            default_entry_id: "fallback".into(),
            timeout_seconds: 12,
        },
        Utc::now(),
    )
    .unwrap();
    let values = plan
        .changes
        .iter()
        .map(|c| (c.target, c.expected.clone()))
        .collect();
    let directory = std::env::temp_dir().join(format!("bootpilot-transactions-{}", Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    (
        FakeSystem {
            snapshot: s,
            values,
            writes: 0,
            fail: None,
            race: false,
            foreign: false,
            concurrency: None,
        },
        Store {
            plan,
            backup: None,
            fail_backup: false,
            fail_step: None,
            fail_finish: false,
            directory,
        },
    )
}
fn apply(s: &mut FakeSystem, store: &mut Store) -> BootResult<ApplyReport> {
    {
        let id = store.plan.id;
        apply_plan(s, store, id, Utc::now())
    }
}
#[test]
fn expired_or_consumed_plan_cannot_apply() {
    let (mut s, mut st) = fixture();
    st.plan.expires_at = Utc::now() - Duration::seconds(1);
    assert_eq!(
        apply(&mut s, &mut st).unwrap_err().code,
        ErrorCode::PlanExpired
    );
    assert_eq!(s.writes, 0);
    st.plan.expires_at = Utc::now() + Duration::minutes(1);
    st.plan.state = PlanState::Started;
    assert_eq!(
        apply(&mut s, &mut st).unwrap_err().code,
        ErrorCode::PlanConsumed
    );
    assert_eq!(s.writes, 0);
}
#[test]
fn changed_snapshot_writes_nothing() {
    let (mut s, mut st) = fixture();
    s.values.insert(
        Target::GrubUser,
        TargetValue::FileBytes(b"foreign".to_vec()),
    );
    assert!(apply(&mut s, &mut st).is_err());
    assert_eq!(s.writes, 0);
}
#[test]
fn change_between_preflight_and_write_is_rejected() {
    let (mut s, mut st) = fixture();
    s.race = true;
    let r = apply(&mut s, &mut st).unwrap();
    assert_eq!(r.outcome, ApplyOutcome::Partial);
    assert_eq!(
        s.values[&Target::GrubMain],
        TargetValue::FileBytes(b"foreign".to_vec())
    );
}
#[test]
fn backup_failure_writes_nothing() {
    let (mut s, mut st) = fixture();
    st.fail_backup = true;
    assert_eq!(
        apply(&mut s, &mut st).unwrap_err().code,
        ErrorCode::BackupFailed
    );
    assert_eq!(s.writes, 0);
}
#[test]
fn second_write_failure_rolls_back_first() {
    let (mut s, mut st) = fixture();
    let original = s.values.clone();
    s.fail = Some(2);
    let r = apply(&mut s, &mut st).unwrap();
    assert_eq!(r.outcome, ApplyOutcome::RolledBack);
    assert_eq!(s.values, original);
    assert!(!r.boot_tested);
}
#[test]
fn rollback_refuses_intervening_change() {
    let (mut s, mut st) = fixture();
    s.fail = Some(2);
    s.foreign = true;
    let r = apply(&mut s, &mut st).unwrap();
    assert_eq!(r.outcome, ApplyOutcome::Partial);
    assert_eq!(
        s.values[&Target::GrubMain],
        TargetValue::FileBytes(b"foreign".to_vec())
    );
}
#[test]
fn incomplete_journal_is_visible_after_restart() {
    let (_, mut st) = fixture();
    let p = st.plan.clone();
    st.begin_backup(
        &p,
        &p.changes
            .iter()
            .map(|c| (c.target, c.expected.clone()))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    st.record_step(
        p.id,
        &JournalStep {
            sequence: 1,
            target: Target::GrubMain,
            phase: StepPhase::Intent,
            at: Utc::now(),
            detail: None,
        },
    )
    .unwrap();
    st.backup = None;
    st.backup =
        serde_json::from_slice(&std::fs::read(st.directory.join("journal.json")).unwrap()).unwrap();
    assert_eq!(st.list_backups().unwrap()[0].journal.len(), 1);
    assert!(st.list_backups().unwrap()[0].outcome.is_none());
}
#[test]
fn restore_rejects_changed_post_state() {
    let (mut s, mut st) = fixture();
    apply(&mut s, &mut st).unwrap();
    let current = s.read_snapshot().unwrap();
    let b = st.backup.clone().unwrap();
    let p = prepare_restore(&current, &b, Utc::now()).unwrap();
    assert_eq!(p.changes[0].replacement, b.targets[0].original);
    s.values.insert(
        Target::GrubUser,
        TargetValue::FileBytes(b"foreign".to_vec()),
    );
    assert!(prepare_restore(&s.read_snapshot().unwrap(), &b, Utc::now()).is_err());
}
#[test]
fn gui_mcp_writes_are_serialized() {
    let active = Arc::new(AtomicUsize::new(0));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let active = active.clone();
            std::thread::spawn(move || {
                let (mut s, mut st) = fixture();
                s.concurrency = Some(active);
                assert_eq!(
                    apply(&mut s, &mut st).unwrap().outcome,
                    ApplyOutcome::Applied
                );
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
}
#[test]
fn forged_replacement_is_rejected() {
    let (mut s, mut st) = fixture();
    st.plan.changes[0].replacement = TargetValue::FileBytes(b"malicious".to_vec());
    assert!(apply(&mut s, &mut st).is_err());
    assert_eq!(s.writes, 0);
}
#[test]
fn successful_plan_is_consumed_and_new_scan_uuid_is_accepted() {
    let (mut s, mut st) = fixture();
    assert_eq!(
        apply(&mut s, &mut st).unwrap().outcome,
        ApplyOutcome::Applied
    );
    assert_eq!(
        apply(&mut s, &mut st).unwrap_err().code,
        ErrorCode::PlanConsumed
    );
}

#[test]
fn interrupted_after_first_write_offers_exact_targeted_restore() {
    let (mut system, mut store) = fixture();
    let plan = store.plan.clone();
    let original = system.values.clone();
    let backup = store
        .begin_backup(
            &plan,
            &plan
                .changes
                .iter()
                .map(|c| (c.target, c.expected.clone()))
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let first = &plan.changes[0];
    store
        .record_step(
            backup.id,
            &JournalStep {
                sequence: 1,
                target: first.target,
                phase: StepPhase::Intent,
                at: Utc::now(),
                detail: None,
            },
        )
        .unwrap();
    system
        .values
        .insert(first.target, first.replacement.clone());
    // Simulate restart after the write but before Written/Verified were persisted.
    let restore = prepare_restore(
        &system.read_snapshot().unwrap(),
        &store.load_backup(backup.id).unwrap(),
        Utc::now(),
    )
    .unwrap();
    store.save_plan(&restore).unwrap();
    assert_eq!(
        apply(&mut system, &mut store).unwrap().outcome,
        ApplyOutcome::Applied
    );
    assert_eq!(system.values, original);
}

#[test]
fn interrupted_partial_bytes_require_manual_recovery() {
    let (mut system, mut store) = fixture();
    apply(&mut system, &mut store).unwrap();
    system.values.insert(
        Target::GrubMain,
        TargetValue::FileBytes(b"partial written content".to_vec()),
    );
    assert_eq!(
        prepare_restore(
            &system.read_snapshot().unwrap(),
            &store.backup.clone().unwrap(),
            Utc::now()
        )
        .unwrap_err()
        .code,
        ErrorCode::Conflict
    );
}

#[test]
fn backup_checksum_tampering_is_rejected() {
    let (mut system, mut store) = fixture();
    apply(&mut system, &mut store).unwrap();
    let mut backup = store.backup.clone().unwrap();
    backup.targets[0].original_checksum = "forged".into();
    assert_eq!(
        prepare_restore(&system.read_snapshot().unwrap(), &backup, Utc::now())
            .unwrap_err()
            .code,
        ErrorCode::InvalidBackup
    );
}

#[test]
fn intent_persistence_failure_never_reaches_target() {
    let (mut system, mut store) = fixture();
    store.fail_step = Some(1);
    assert_eq!(
        apply(&mut system, &mut store).unwrap().outcome,
        ApplyOutcome::Partial
    );
    assert_eq!(system.writes, 0);
}
#[test]
fn journal_failure_after_write_rolls_back_and_preserves_failure_detail() {
    let (mut system, mut store) = fixture();
    let original = system.values.clone();
    store.fail_step = Some(2);
    let result = apply(&mut system, &mut store).unwrap();
    assert_eq!(result.outcome, ApplyOutcome::RolledBack);
    assert_eq!(system.values, original);
    assert!(
        result
            .verified_steps
            .iter()
            .any(|s| s.phase == StepPhase::Failed && s.detail.is_some())
    );
}
#[test]
fn failed_outcome_persistence_exposes_report_and_unfinished_journal() {
    let (mut system, mut store) = fixture();
    store.fail_finish = true;
    let error = apply(&mut system, &mut store).unwrap_err();
    assert_eq!(error.details["report"]["outcome"], "applied");
    assert!(store.list_backups().unwrap()[0].outcome.is_none());
    assert_eq!(store.plan.state, PlanState::Started);
}
#[test]
fn temporary_file_transaction_and_restore_round_trip() {
    struct DiskSystem {
        inner: FakeSystem,
        directory: std::path::PathBuf,
    }
    impl DiskSystem {
        fn path(&self, t: &Target) -> std::path::PathBuf {
            self.directory.join(format!("{t:?}.cfg"))
        }
    }
    impl SystemAccess for DiskSystem {
        fn read_snapshot(&mut self) -> BootResult<Snapshot> {
            for t in [Target::GrubMain, Target::GrubUser] {
                let value = self.read_target(&t)?;
                self.inner.values.insert(t, value);
            }
            self.inner.read_snapshot()
        }
        fn read_target(&mut self, t: &Target) -> BootResult<TargetValue> {
            Ok(TargetValue::FileBytes(std::fs::read(self.path(t)).unwrap()))
        }
        fn write_target(&mut self, t: &Target, e: &TargetValue, r: &TargetValue) -> BootResult<()> {
            if self.read_target(t)? != *e {
                return Err(err(ErrorCode::Conflict));
            }
            let TargetValue::FileBytes(bytes) = r else {
                panic!()
            };
            std::fs::write(self.path(t), bytes).unwrap();
            std::fs::OpenOptions::new()
                .write(true)
                .open(self.path(t))
                .unwrap()
                .sync_all()
                .unwrap();
            Ok(())
        }
    }
    let (inner, mut store) = fixture();
    let original = inner.values.clone();
    let mut system = DiskSystem {
        inner,
        directory: store.directory.clone(),
    };
    for (target, value) in &original {
        let TargetValue::FileBytes(bytes) = value else {
            panic!()
        };
        std::fs::write(system.path(target), bytes).unwrap();
    }
    let id = store.plan.id;
    assert_eq!(
        apply_plan(&mut system, &mut store, id, Utc::now())
            .unwrap()
            .outcome,
        ApplyOutcome::Applied
    );
    let restore = prepare_restore(
        &system.read_snapshot().unwrap(),
        &store.backup.clone().unwrap(),
        Utc::now(),
    )
    .unwrap();
    store.save_plan(&restore).unwrap();
    assert_eq!(
        apply_plan(&mut system, &mut store, restore.id, Utc::now())
            .unwrap()
            .outcome,
        ApplyOutcome::Applied
    );
    for (target, value) in original {
        assert_eq!(system.read_target(&target).unwrap(), value);
    }
}
