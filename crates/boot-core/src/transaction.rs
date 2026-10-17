//! Guarded transactions. The native broker additionally holds the machine-wide lock.
use crate::*;
use chrono::{DateTime, Duration, Utc};
use std::sync::Mutex;
use uuid::Uuid;

static TRANSACTION_LOCK: Mutex<()> = Mutex::new(());
fn error(code: ErrorCode, message: &str) -> BootError {
    BootError::new(code, message)
}

pub fn apply_plan(
    system: &mut dyn SystemAccess,
    store: &mut dyn PlanStore,
    plan_id: Uuid,
    now: DateTime<Utc>,
) -> BootResult<ApplyReport> {
    let _lock = TRANSACTION_LOCK.lock().map_err(|_| {
        error(
            ErrorCode::Conflict,
            "Vorherige Transaktion wurde unterbrochen; Helfer neu starten.",
        )
    })?;
    let plan = store.load_plan(plan_id)?;
    if plan.id != plan_id || plan.schema_version != SCHEMA_VERSION {
        return Err(error(
            ErrorCode::InvalidSchema,
            "Ungültiger gespeicherter Plan.",
        ));
    }
    if plan.state != PlanState::Ready {
        return Err(error(
            ErrorCode::PlanConsumed,
            "Plan wurde bereits begonnen.",
        ));
    }
    if plan.expires_at <= now
        || plan.created_at > now
        || plan.expires_at - plan.created_at > Duration::seconds(PLAN_LIFETIME_SECONDS)
    {
        return Err(error(
            ErrorCode::PlanExpired,
            "Plan ist abgelaufen oder sein Zeitfenster ist ungültig.",
        ));
    }
    let snapshot = system.read_snapshot()?;
    let regenerated = match plan.request.clone() {
        PlanRequest::Restore { backup_id } => {
            prepare_restore(&snapshot, &store.load_backup(backup_id)?, now)?
        }
        request => prepare_plan(&snapshot, request, now)?,
    };
    // Fresh collection IDs deliberately differ. Only authoritative values and the closed request matter.
    if plan.changes != regenerated.changes
        || plan.preconditions != regenerated.preconditions
        || plan.diffs != regenerated.diffs
    {
        return Err(error(
            ErrorCode::Conflict,
            "Plan stimmt nicht mehr mit der unabhängig geprüften Vorschau überein.",
        ));
    }
    let mut originals = Vec::new();
    for change in &plan.changes {
        let value = system.read_target(&change.target)?;
        if value != change.expected {
            return Err(error(
                ErrorCode::Conflict,
                "Bootzustand wurde seit der Vorschau verändert.",
            ));
        }
        originals.push((change.target, value));
    }
    let backup = store.begin_backup(&plan, &originals)?;
    let mut report = ApplyReport {
        schema_version: SCHEMA_VERSION,
        plan_id,
        backup_id: Some(backup.id),
        outcome: ApplyOutcome::Applied,
        verified_steps: vec![],
        remaining_deviations: vec![],
        boot_tested: false,
    };
    let mut sequence = 0;
    let mut recoverable = Vec::new();
    let mut failed = false;
    for change in &plan.changes {
        if change.expected == change.replacement {
            continue;
        }
        if let Err(e) = record(
            store,
            backup.id,
            &mut sequence,
            change.target,
            StepPhase::Intent,
            None,
            &mut report,
        ) {
            report
                .remaining_deviations
                .push(deviation(change, e.to_string(), None));
            failed = true;
            break;
        }
        let result = (|| {
            match system.write_target(&change.target, &change.expected, &change.replacement) {
                Ok(()) => recoverable.push(change),
                Err(failure) => {
                    let phase = match &failure {
                        WriteFailure::NotWritten(_) => {
                            // Even equality with replacement cannot make a foreign write ours.
                            let current = system.read_target(&change.target).ok();
                            if current.as_ref() != Some(&change.expected) {
                                report.remaining_deviations.push(deviation(
                                    change,
                                    "Abgewiesener Schreibversuch; fremder Zielzustand bleibt erhalten.".into(),
                                    current,
                                ));
                            }
                            StepPhase::NotWritten
                        }
                        WriteFailure::MayHaveWritten(_) => {
                            recoverable.push(change);
                            StepPhase::WriteUncertain
                        }
                    };
                    let error = failure.into_error();
                    // Missing receipts never authorize later restore (Intent alone is insufficient).
                    if let Err(storage_error) = record(
                        store,
                        backup.id,
                        &mut sequence,
                        change.target,
                        phase,
                        Some(error.to_string()),
                        &mut report,
                    ) {
                        return Err(
                            storage_error.with_details(serde_json::json!({"write_error": error}))
                        );
                    }
                    return Err(error);
                }
            }
            record(
                store,
                backup.id,
                &mut sequence,
                change.target,
                StepPhase::Written,
                None,
                &mut report,
            )?;
            if system.read_target(&change.target)? != change.replacement {
                return Err(error(
                    ErrorCode::VerificationFailed,
                    "Geschriebener Wert wurde nicht bestätigt.",
                ));
            }
            record(
                store,
                backup.id,
                &mut sequence,
                change.target,
                StepPhase::Verified,
                None,
                &mut report,
            )
        })();
        if let Err(e) = result {
            let step = JournalStep {
                sequence: sequence + 1,
                target: change.target,
                phase: StepPhase::Failed,
                at: Utc::now(),
                detail: Some(e.to_string()),
            };
            sequence += 1;
            // Keep failure visible even if persistent storage itself failed.
            let _ = store.record_step(backup.id, &step);
            report.verified_steps.push(step);
            failed = true;
            break;
        }
    }
    if failed {
        for change in recoverable.into_iter().rev() {
            let current = system.read_target(&change.target);
            if current.as_ref().ok() == Some(&change.expected) {
                continue;
            }
            if current.as_ref().ok() != Some(&change.replacement) {
                report.remaining_deviations.push(deviation(
                    change,
                    "Fremde oder unbekannte Änderung wird nicht überschrieben.".into(),
                    current.ok(),
                ));
                continue;
            }
            let result = (|| {
                record(
                    store,
                    backup.id,
                    &mut sequence,
                    change.target,
                    StepPhase::RollbackIntent,
                    None,
                    &mut report,
                )?;
                system
                    .write_target(&change.target, &change.replacement, &change.expected)
                    .map_err(WriteFailure::into_error)?;
                if system.read_target(&change.target)? != change.expected {
                    return Err(error(
                        ErrorCode::RollbackFailed,
                        "Rücknahme konnte nicht bestätigt werden.",
                    ));
                }
                record(
                    store,
                    backup.id,
                    &mut sequence,
                    change.target,
                    StepPhase::RolledBack,
                    None,
                    &mut report,
                )
            })();
            if let Err(e) = result {
                report.remaining_deviations.push(deviation(
                    change,
                    e.to_string(),
                    system.read_target(&change.target).ok(),
                ));
            }
        }
        report.outcome = if report.remaining_deviations.is_empty() {
            ApplyOutcome::RolledBack
        } else {
            ApplyOutcome::Partial
        };
    }
    if let Err(e) = store.finish_plan(backup.id, &report) {
        return Err(e.clone().with_details(
            serde_json::json!({"storage_error":e,"report":report,"incomplete_journal":backup.id}),
        ));
    }
    Ok(report)
}
fn deviation(
    change: &PlannedChange,
    description: String,
    actual: Option<TargetValue>,
) -> Deviation {
    Deviation {
        target: change.target,
        description,
        expected: Some(change.expected.clone()),
        actual,
    }
}
fn record(
    store: &mut dyn PlanStore,
    id: Uuid,
    sequence: &mut u32,
    target: Target,
    phase: StepPhase,
    detail: Option<String>,
    report: &mut ApplyReport,
) -> BootResult<()> {
    *sequence += 1;
    let step = JournalStep {
        sequence: *sequence,
        target,
        phase,
        at: Utc::now(),
        detail,
    };
    store.record_step(id, &step)?;
    report.verified_steps.push(step);
    Ok(())
}
fn snapshot_value(snapshot: &Snapshot, target: Target) -> BootResult<TargetValue> {
    match target {
        Target::FirmwareOrder
            if snapshot.sources.bcd == CollectionState::Complete
                && snapshot.sources.firmware == CollectionState::Complete =>
        {
            Ok(TargetValue::FirmwareOrder(snapshot.firmware_order.clone()))
        }
        Target::GrubMain | Target::GrubUser
            if snapshot.sources.grub == CollectionState::Complete =>
        {
            snapshot
                .grub
                .as_ref()
                .map(|g| {
                    TargetValue::FileBytes(if target == Target::GrubMain {
                        g.main_bytes.clone()
                    } else {
                        g.user_bytes.clone()
                    })
                })
                .ok_or(error(ErrorCode::IncompleteSnapshot, "GRUB-Dateien fehlen."))
        }
        _ => Err(error(
            ErrorCode::IncompleteSnapshot,
            "Zielzustand ist nicht vollständig erfasst.",
        )),
    }
}
pub fn prepare_restore(
    snapshot: &Snapshot,
    backup: &BackupRecord,
    now: DateTime<Utc>,
) -> BootResult<ChangePlan> {
    if backup.schema_version != SCHEMA_VERSION
        || backup.targets.is_empty()
        || backup.targets.len() > 3
    {
        return Err(error(ErrorCode::InvalidBackup, "Ungültige Sicherung."));
    }
    let mut changes = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for target in &backup.targets {
        let compatible = matches!(
            (&target.target, &target.original, &target.applied),
            (
                Target::FirmwareOrder,
                TargetValue::FirmwareOrder(_),
                TargetValue::FirmwareOrder(_)
            ) | (
                Target::GrubMain | Target::GrubUser,
                TargetValue::FileBytes(_),
                TargetValue::FileBytes(_)
            )
        );
        if !compatible
            || !seen.insert(target.target)
            || target.original_checksum != target_checksum(&target.original)
            || target.applied_checksum != target_checksum(&target.applied)
        {
            return Err(error(
                ErrorCode::InvalidBackup,
                "Sicherungsprüfsumme oder Zieltyp ist ungültig.",
            ));
        }
        let current = snapshot_value(snapshot, target.target)?;
        // Unchanged / already rolled back targets remain explicit preconditions.
        if current != target.original && current != target.applied {
            return Err(error(
                ErrorCode::Conflict,
                "Ziel wurde nach dieser Sicherung fremd verändert.",
            ));
        }
        // Intent can precede a rejected write or process death. A positive receipt is
        // required, and a later rollback/rejection invalidates that ownership evidence.
        let receipt = backup
            .journal
            .iter()
            .filter(|s| s.target == target.target && s.phase != StepPhase::Failed)
            .max_by_key(|s| s.sequence);
        if current != target.original
            && !receipt.is_some_and(|s| {
                matches!(
                    s.phase,
                    StepPhase::Written | StepPhase::Verified | StepPhase::WriteUncertain
                )
            })
        {
            return Err(error(
                ErrorCode::InvalidBackup,
                "Kein gültiger Schreibnachweis für dieses Ziel; manuelle Prüfung erforderlich.",
            ));
        }
        if let TargetValue::FirmwareOrder(ids) = &target.original {
            validate_request(
                &PlanRequest::Order {
                    entry_ids: ids.clone(),
                },
                snapshot,
            )?;
        }
        changes.push(PlannedChange {
            target: target.target,
            expected: current,
            replacement: target.original.clone(),
        });
    }
    if changes.iter().all(|c| c.expected == c.replacement) {
        return Err(error(
            ErrorCode::InvalidBackup,
            "Diese Sicherung enthält keine verbleibenden eigenen Änderungen.",
        ));
    }
    // The resulting pair must still be a supported, synchronized GRUB document.
    if seen.contains(&Target::GrubMain) || seen.contains(&Target::GrubUser) {
        if !seen.contains(&Target::GrubMain) || !seen.contains(&Target::GrubUser) {
            return Err(error(
                ErrorCode::InvalidBackup,
                "GRUB-Sicherung ist unvollständig.",
            ));
        }
        let bytes = |t| {
            changes
                .iter()
                .find(|c| c.target == t)
                .and_then(|c| match &c.replacement {
                    TargetValue::FileBytes(b) => Some(b.as_slice()),
                    _ => None,
                })
                .unwrap()
        };
        if !parse_grub(bytes(Target::GrubMain), bytes(Target::GrubUser))?.write_supported {
            return Err(error(
                ErrorCode::InvalidBackup,
                "Gesicherte GRUB-Dateien werden nicht unterstützt.",
            ));
        }
    }
    Ok(ChangePlan {
        schema_version: SCHEMA_VERSION,
        id: Uuid::new_v4(),
        created_at: now,
        expires_at: now + Duration::seconds(PLAN_LIFETIME_SECONDS),
        snapshot_id: snapshot.id,
        request: PlanRequest::Restore {
            backup_id: backup.id,
        },
        diffs: changes
            .iter()
            .map(|c| ChangeDiff {
                target: c.target,
                description: "Gezielte Rücknahme der noch vorhandenen eigenen Änderung.".into(),
                before: display(&c.expected),
                after: display(&c.replacement),
            })
            .collect(),
        preconditions: changes
            .iter()
            .map(|c| Precondition {
                target: c.target,
                expected_checksum: target_checksum(&c.expected),
            })
            .collect(),
        changes,
        state: PlanState::Ready,
    })
}
fn display(v: &TargetValue) -> String {
    match v {
        TargetValue::FileBytes(b) => String::from_utf8_lossy(b).into_owned(),
        TargetValue::FirmwareOrder(ids) => ids
            .iter()
            .map(Uuid::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
