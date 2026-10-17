use crate::*;
use std::collections::BTreeSet;

pub fn validate_request(request: &PlanRequest, snapshot: &Snapshot) -> BootResult<()> {
    if snapshot.schema_version != SCHEMA_VERSION {
        return Err(BootError::new(
            ErrorCode::InvalidSchema,
            "Unbekannte Erfassungsversion.",
        ));
    }
    match request {
        PlanRequest::Order { entry_ids } => validate_order(entry_ids, snapshot),
        PlanRequest::Menu {
            default_entry_id,
            timeout_seconds,
        } => {
            let grub = writable_grub(snapshot)?;
            if !(1..=300).contains(timeout_seconds) {
                return Err(BootError::new(
                    ErrorCode::InvalidRequest,
                    "Die Wartezeit muss zwischen 1 und 300 Sekunden liegen.",
                ));
            }
            if grub
                .entries
                .iter()
                .filter(|entry| entry.id == *default_entry_id)
                .count()
                != 1
            {
                return Err(BootError::new(
                    ErrorCode::UnknownEntry,
                    "Der Menüeintrag fehlt oder ist nicht eindeutig.",
                ));
            }
            Ok(())
        }
        PlanRequest::Cachyos {
            candidate_id,
            title,
        } => {
            writable_grub(snapshot)?;
            require_complete(snapshot.sources.linux)?;
            let candidates: Vec<_> = snapshot
                .linux_candidates
                .iter()
                .filter(|candidate| candidate.id == *candidate_id)
                .collect();
            if candidate_id.is_nil() || candidates.len() != 1 || !candidates[0].validated {
                return Err(BootError::new(
                    ErrorCode::InvalidCandidate,
                    "Der Linux-Kandidat ist nicht eindeutig geprüft.",
                ));
            }
            if title.trim().is_empty()
                || title.chars().count() > 120
                || title.chars().any(char::is_control)
            {
                return Err(BootError::new(
                    ErrorCode::InvalidTitle,
                    "Der Titel muss 1 bis 120 Zeichen ohne Steuerzeichen enthalten.",
                ));
            }
            Ok(())
        }
        PlanRequest::Restore { backup_id } => {
            if backup_id.is_nil() {
                return Err(BootError::new(
                    ErrorCode::InvalidBackup,
                    "Die Sicherungs-ID ist ungültig.",
                ));
            }
            // Ownership, checksums and post-state need the authoritative PlanStore (Task 4).
            Ok(())
        }
    }
}

fn require_complete(state: CollectionState) -> BootResult<()> {
    if state != CollectionState::Complete {
        return Err(BootError::new(
            ErrorCode::IncompleteSnapshot,
            "Die erforderliche Quelle wurde nicht vollständig erfasst.",
        ));
    }
    Ok(())
}

fn writable_grub(snapshot: &Snapshot) -> BootResult<&GrubDocument> {
    require_complete(snapshot.sources.grub)?;
    snapshot
        .grub
        .as_ref()
        .filter(|grub| grub.write_supported)
        .ok_or_else(|| {
            BootError::new(
                ErrorCode::UnsupportedConfiguration,
                "Die GRUB-Konfiguration kann nicht sicher geändert werden.",
            )
        })
}

fn validate_order(entry_ids: &[uuid::Uuid], snapshot: &Snapshot) -> BootResult<()> {
    require_complete(snapshot.sources.firmware)?;
    let known: BTreeSet<_> = snapshot
        .firmware_entries
        .iter()
        .map(|entry| entry.id)
        .collect();
    let current: BTreeSet<_> = snapshot.firmware_order.iter().copied().collect();
    let requested: BTreeSet<_> = entry_ids.iter().copied().collect();
    if known.is_empty()
        || known.len() != snapshot.firmware_entries.len()
        || current.len() != snapshot.firmware_order.len()
        || current != known
        || known.iter().any(uuid::Uuid::is_nil)
    {
        return Err(BootError::new(
            ErrorCode::IncompleteSnapshot,
            "Das Firmware-Inventar ist leer oder widersprüchlich.",
        ));
    }
    if requested.len() != entry_ids.len() || requested != known {
        return Err(BootError::new(
            ErrorCode::InvalidRequest,
            "Die Reihenfolge muss jeden bekannten Firmware-Eintrag genau einmal enthalten.",
        ));
    }
    Ok(())
}

pub fn diagnose(snapshot: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    match (snapshot.sources.boot_current, &snapshot.boot_current) {
        (CollectionState::Complete, Some(current)) => findings.push(Finding {
            code: "boot_current_observed".into(), severity: Severity::Info,
            certainty: FindingCertainty::Evidence,
            statement: format!("BootCurrent meldet den UEFI-Eintrag Boot{:04X}.", current.uefi_number),
            evidence: vec![evidence(Source::BootCurrent, "boot_current")], open_check: None,
        }),
        _ => findings.push(Finding {
            code: "boot_current_unknown".into(), severity: Severity::Warning,
            certainty: FindingCertainty::Evidence,
            statement: "BootCurrent wurde nicht vollständig erfasst; der verwendete Firmware-Eintrag ist unbekannt.".into(),
            evidence: vec![evidence(Source::BootCurrent, "sources.boot_current"), evidence(Source::BootCurrent, "boot_current")],
            open_check: Some("BootCurrent mit ausreichenden Leserechten erfassen; daraus folgt kein Nachweis eines übersprungenen Bootloaders.".into()),
        }),
    }
    for (source, state, value, name, field) in [
        (
            Source::SecureBoot,
            snapshot.sources.secure_boot,
            snapshot.secure_boot,
            "Secure Boot",
            "secure_boot",
        ),
        (
            Source::Bitlocker,
            snapshot.sources.bitlocker,
            snapshot.bitlocker,
            "BitLocker",
            "bitlocker",
        ),
    ] {
        let statement = match (state, value) {
            (CollectionState::Complete, Some(true)) => {
                format!("{name} ist laut Erfassung aktiviert.")
            }
            (CollectionState::Complete, Some(false)) => {
                format!("{name} ist laut Erfassung deaktiviert.")
            }
            _ => format!("Der Status von {name} ist unbekannt."),
        };
        findings.push(Finding {
            code: format!("{field}_status"),
            severity: Severity::Info,
            certainty: FindingCertainty::Evidence,
            statement,
            evidence: vec![
                evidence(source, field),
                evidence(source, &format!("sources.{field}")),
            ],
            open_check: if state == CollectionState::Complete && value.is_some() {
                None
            } else {
                Some(format!(
                    "{name}-Status mit geeigneten Leserechten erfassen."
                ))
            },
        });
    }
    for (index, failure) in snapshot.source_errors.iter().enumerate() {
        findings.push(Finding {
            code: "source_error".into(), severity: Severity::Warning,
            certainty: FindingCertainty::Evidence, statement: failure.error.message.clone(),
            evidence: vec![evidence(failure.source, &format!("source_errors[{index}]"))],
            open_check: Some("Die genannte Erfassungsquelle erneut prüfen; fehlende Daten belegen kein fehlendes Betriebssystem.".into()),
        });
    }
    findings
}

fn evidence(source: Source, reference: &str) -> EvidenceRef {
    EvidenceRef {
        source,
        reference: reference.into(),
    }
}
