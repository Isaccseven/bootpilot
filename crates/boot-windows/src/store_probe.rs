//! Pure fake-target test with the real protected store. No inventory or native write calls.
use boot_core::*;
use chrono::Utc;
use uuid::Uuid;
struct Fake {
    snapshot: Snapshot,
    writes: usize,
}
impl SystemAccess for Fake {
    fn read_snapshot(&mut self) -> BootResult<Snapshot> {
        let mut s = self.snapshot.clone();
        s.id = Uuid::new_v4();
        Ok(s)
    }
    fn read_target(&mut self, t: &Target) -> BootResult<TargetValue> {
        assert_eq!(*t, Target::FirmwareOrder);
        Ok(TargetValue::FirmwareOrder(
            self.snapshot.firmware_order.clone(),
        ))
    }
    fn write_target(&mut self, t: &Target, e: &TargetValue, r: &TargetValue) -> TargetWriteResult {
        if self.read_target(t).map_err(WriteFailure::NotWritten)? != *e {
            return Err(WriteFailure::NotWritten(crate::error(
                ErrorCode::Conflict,
                "Fake-Ziel verändert",
            )));
        }
        let TargetValue::FirmwareOrder(ids) = r else {
            return Err(WriteFailure::NotWritten(crate::error(
                ErrorCode::InvalidRequest,
                "Fake-Typ",
            )));
        };
        self.snapshot.firmware_order = ids.clone();
        self.writes += 1;
        Ok(())
    }
}
fn check(condition: bool, message: &str) -> BootResult<()> {
    if condition {
        Ok(())
    } else {
        Err(crate::error(ErrorCode::VerificationFailed, message))
    }
}
/// A separate process reads only previously created test records.
pub fn verify(run_id: Uuid) -> BootResult<serde_json::Value> {
    let caller = format!("isolated-store-test-{run_id}");
    let store = crate::ProtectedStore::open(&caller)?;
    let backups = store.list_backups()?;
    check(
        backups.len() == 2,
        "Exactly two prior test backups required",
    )?;
    let complete = backups
        .iter()
        .find(|b| b.outcome.is_some())
        .ok_or_else(|| crate::error(ErrorCode::VerificationFailed, "Completed journal missing"))?;
    let incomplete = backups
        .iter()
        .find(|b| b.outcome.is_none())
        .ok_or_else(|| crate::error(ErrorCode::VerificationFailed, "Incomplete journal missing"))?;
    check(
        store.load_plan(complete.plan_id)?.state == PlanState::Finished
            && complete.journal.len() == 3
            && complete.outcome.as_ref().unwrap().outcome == ApplyOutcome::Applied,
        "Completed plan/journal invalid",
    )?;
    check(
        store.load_plan(incomplete.plan_id)?.state == PlanState::Started
            && incomplete.journal.len() == 1
            && incomplete.journal[0].phase == StepPhase::Intent,
        "Started plan/incomplete journal invalid",
    )?;
    for b in &backups {
        for target in &b.targets {
            check(
                target.original_checksum == target_checksum(&target.original)
                    && target.applied_checksum == target_checksum(&target.applied),
                "Stored checksum mismatch",
            )?;
        }
    }
    Ok(
        serde_json::json!({"passed":true,"run_id":run_id,"separate_process_reload":true,"finished_plan":true,"started_plan":true,"completed_journal":true,"incomplete_journal":true,"checksums":true,"fake_target_writes":0,"real_boot_reads":0,"real_boot_writes":0}),
    )
}
pub fn run(run_id: Uuid) -> BootResult<serde_json::Value> {
    #[cfg(windows)]
    let _lock = crate::write::MachineLock::acquire()?;
    let caller = format!("isolated-store-test-{run_id}");
    let mut store = crate::ProtectedStore::open(&caller)?;
    check(store.list_backups()?.is_empty(), "Testlauf ist nicht leer")?;
    let mut snapshot = Snapshot::new(Uuid::new_v4(), Utc::now());
    snapshot.sources.firmware = CollectionState::Complete;
    snapshot.sources.bcd = CollectionState::Complete;
    snapshot.firmware_order = vec![Uuid::new_v4(), Uuid::new_v4()];
    snapshot.firmware_entries = snapshot
        .firmware_order
        .iter()
        .map(|id| FirmwareEntry {
            id: *id,
            title: "Synthetic fixture, no boot target".into(),
            loader_path: None,
        })
        .collect();
    let mut reversed = snapshot.firmware_order.clone();
    reversed.reverse();
    let plan = prepare_plan(
        &snapshot,
        PlanRequest::Order {
            entry_ids: reversed,
        },
        Utc::now(),
    )?;
    store.save_plan(&plan)?;
    check(
        store.save_plan(&plan).is_err(),
        "Unveränderlicher Plan konnte überschrieben werden",
    )?;
    drop(store);
    let mut store = crate::ProtectedStore::open(&caller)?;
    check(
        store.load_plan(plan.id)? == plan,
        "Plan wurde nicht unverändert wieder geladen",
    )?;
    let mut fake = Fake {
        snapshot,
        writes: 0,
    };
    let result = apply_plan(&mut fake, &mut store, plan.id, Utc::now())?;
    check(
        result.outcome == ApplyOutcome::Applied && fake.writes == 1,
        "Fake-Transaktion fehlgeschlagen",
    )?;
    drop(store);
    let mut store = crate::ProtectedStore::open(&caller)?;
    check(
        store.load_plan(plan.id)?.state == PlanState::Finished,
        "Planverbrauch nicht dauerhaft",
    )?;
    let backups = store.list_backups()?;
    check(
        backups.len() == 1 && backups[0].journal.len() == 3 && backups[0].outcome == Some(result),
        "Journal/Ergebnis nicht dauerhaft",
    )?;
    check(
        apply_plan(&mut fake, &mut store, plan.id, Utc::now())
            .unwrap_err()
            .code
            == ErrorCode::PlanConsumed,
        "Verbrauchter Plan wiederholt",
    )?;
    let restore = prepare_restore(&fake.snapshot, &backups[0], Utc::now())?;
    store.save_plan(&restore)?;
    let b = store.begin_backup(
        &restore,
        &restore
            .changes
            .iter()
            .map(|c| (c.target, c.expected.clone()))
            .collect::<Vec<_>>(),
    )?;
    store.record_step(
        b.id,
        &JournalStep {
            sequence: 1,
            target: Target::FirmwareOrder,
            phase: StepPhase::Intent,
            at: Utc::now(),
            detail: Some("Simulated interruption; no target write".into()),
        },
    )?;
    drop(store);
    let store = crate::ProtectedStore::open(&caller)?;
    let incomplete = store.load_backup(b.id)?;
    check(
        incomplete.outcome.is_none()
            && incomplete.journal.len() == 1
            && store.list_backups()?.len() == 2,
        "Unvollständiges Journal nach Wiederöffnung nicht sichtbar",
    )?;
    check(
        store.load_plan(restore.id)?.state == PlanState::Started,
        "Begonnener Plan nicht dauerhaft verbraucht",
    )?;
    Ok(
        serde_json::json!({"run_id":run_id,"passed":true,"real_boot_reads":0,"real_boot_writes":0,"fake_target_writes":fake.writes,"immutable_plan":true,"restart_plan":true,"restart_completed_journal":true,"restart_incomplete_journal":true,"machine_mutex":true,"caller_test_record":caller}),
    )
}
