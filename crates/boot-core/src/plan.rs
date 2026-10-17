//! Pure planning. No target reads/writes or elevation occur in this module.
use crate::*;
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// SHA-256 over a domain-separated, length-prefixed canonical target value.
/// File values retain their exact bytes; UUIDs use RFC 4122's 16-byte order.
/// Transaction and helper code must use this same function rather than JSON hashing.
pub fn target_checksum(value: &TargetValue) -> String {
    let mut hash = Sha256::new();
    hash.update(b"bootpilot-target-v1\0");
    match value {
        TargetValue::FileBytes(bytes) => {
            hash.update(b"file\0");
            hash.update((bytes.len() as u64).to_be_bytes());
            hash.update(bytes);
        }
        TargetValue::FirmwareOrder(ids) => {
            hash.update(b"firmware-order\0");
            hash.update((ids.len() as u64).to_be_bytes());
            for id in ids {
                hash.update(id.as_bytes());
            }
        }
    }
    format!("{:x}", hash.finalize())
}

pub fn prepare_plan(
    snapshot: &Snapshot,
    request: PlanRequest,
    now: DateTime<Utc>,
) -> BootResult<ChangePlan> {
    validate_request(&request, snapshot)?;
    let changes = match &request {
        PlanRequest::Order { entry_ids } => vec![PlannedChange {
            target: Target::FirmwareOrder,
            expected: TargetValue::FirmwareOrder(snapshot.firmware_order.clone()),
            replacement: TargetValue::FirmwareOrder(entry_ids.clone()),
        }],
        PlanRequest::Menu {
            default_entry_id,
            timeout_seconds,
        } => {
            let doc = snapshot.grub.as_ref().expect("request validated");
            let (main, user) = crate::grub::edit_menu(doc, default_entry_id, *timeout_seconds)?;
            file_changes(doc, main, user)
        }
        PlanRequest::Cachyos {
            candidate_id,
            title,
        } => {
            let candidate = snapshot
                .linux_candidates
                .iter()
                .find(|c| c.id == *candidate_id)
                .expect("request validated");
            let doc = snapshot.grub.as_ref().expect("request validated");
            let id = format!("bootpilot-cachyos-{}", candidate.id);
            // Reparse original bytes: derived metadata is never authority for an edit.
            if parse_grub(&doc.main_bytes, &doc.user_bytes)?
                .entries
                .iter()
                .any(|e| e.id == id)
            {
                return Err(BootError::new(
                    ErrorCode::InvalidCandidate,
                    "Dieser geprüfte Kandidat hat bereits einen BootPilot-Eintrag.",
                ));
            }
            let entry = format!(
                "\nmenuentry {} --id={} --class=linux {{\n    insmod part_gpt\n    insmod fat\n    insmod search_fs_uuid\n    if search --no-floppy --fs-uuid --set=root {}; then\n        linux {} {}\n        initrd {}\n    else\n        echo 'Die gepruefte Bootpartition wurde nicht gefunden.'\n        sleep --interruptible 15\n    fi\n}}\n",
                quote_title(title),
                id,
                candidate.filesystem_uuid,
                candidate.kernel_path,
                candidate.kernel_parameters.join(" "),
                candidate.initramfs_paths.join(" ")
            );
            let (main, user) = crate::grub::append_entry(doc, &entry)?;
            file_changes(doc, main, user)
        }
        PlanRequest::Restore { .. } => {
            return Err(BootError::new(
                ErrorCode::NotImplemented,
                "Eine Rücknahme benötigt den geprüften Sicherungsspeicher.",
            ));
        }
    };
    let mut diffs = vec![];
    let mut preconditions = vec![];
    for change in &changes {
        preconditions.push(Precondition {
            target: change.target,
            expected_checksum: target_checksum(&change.expected),
        });
        let description = match change.target {
            Target::FirmwareOrder => {
                "Alle vorhandenen Firmware-Einträge bleiben erhalten; nur ihre Reihenfolge ändert sich."
            }
            Target::GrubMain => {
                "Statische GRUB-Menüeinstellungen und synchrone Benutzersektion. Die vorhandene Grub2Win-Laufzeitbibliothek wird vorausgesetzt; fremder Code wird nicht ausgeführt oder vollständig ausgewertet."
            }
            Target::GrubUser => {
                "Benutzersektion synchron zur Hauptdatei; vorhandene Installer und Rückfalleinträge bleiben erhalten."
            }
        };
        diffs.push(ChangeDiff {
            target: change.target,
            description: description.into(),
            before: display_value(&change.expected),
            after: display_value(&change.replacement),
        });
    }
    Ok(ChangePlan {
        schema_version: SCHEMA_VERSION,
        id: Uuid::new_v4(),
        created_at: now,
        expires_at: now + Duration::seconds(PLAN_LIFETIME_SECONDS),
        snapshot_id: snapshot.id,
        request,
        diffs,
        preconditions,
        changes,
        state: PlanState::Ready,
    })
}

fn file_changes(doc: &GrubDocument, main: Vec<u8>, user: Vec<u8>) -> Vec<PlannedChange> {
    // Both targets are included even if one is unchanged: synchronization is a precondition.
    vec![
        PlannedChange {
            target: Target::GrubMain,
            expected: TargetValue::FileBytes(doc.main_bytes.clone()),
            replacement: TargetValue::FileBytes(main),
        },
        PlannedChange {
            target: Target::GrubUser,
            expected: TargetValue::FileBytes(doc.user_bytes.clone()),
            replacement: TargetValue::FileBytes(user),
        },
    ]
}
fn display_value(value: &TargetValue) -> String {
    match value {
        TargetValue::FileBytes(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        TargetValue::FirmwareOrder(ids) => ids
            .iter()
            .map(Uuid::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
fn quote_title(title: &str) -> String {
    format!("'{}'", title.replace('\'', "'\\''"))
}

pub(crate) fn validate_candidate(c: &LinuxCandidate, s: &Snapshot) -> BootResult<()> {
    let volumes: Vec<_> = s
        .volumes
        .iter()
        .filter(|v| v.volume_id == Some(c.volume_id))
        .collect();
    let volume_ok = s.sources.volumes == CollectionState::Complete
        && !c.volume_id.is_nil()
        && volumes.len() == 1
        && volumes[0]
            .filesystem_uuid
            .as_deref()
            .is_some_and(|uuid| uuid.eq_ignore_ascii_case(&c.filesystem_uuid))
        && volumes[0]
            .filesystem
            .as_deref()
            .is_some_and(|f| matches!(f.to_ascii_lowercase().as_str(), "fat" | "fat32" | "vfat"));
    let evidence_ok = [Source::Volumes, Source::Linux].iter().all(|source| {
        c.evidence
            .iter()
            .any(|e| e.source == *source && !e.reference.trim().is_empty())
    });
    let root_count = c
        .kernel_parameters
        .iter()
        .filter(|p| p.starts_with("root=UUID="))
        .count();
    if !c.validated
        || !volume_ok
        || !evidence_ok
        || !safe_uuid(&c.filesystem_uuid)
        || !safe_path(&c.kernel_path)
        || c.initramfs_paths.is_empty()
        || c.initramfs_paths.len() > 4
        || !c.initramfs_paths.iter().all(|p| safe_path(p))
        || c.kernel_parameters.len() > 32
        || root_count != 1
        || !c.kernel_parameters.iter().all(|p| safe_parameter(p))
    {
        return Err(BootError::new(
            ErrorCode::InvalidCandidate,
            "Volume, Startdateien oder Kernelparameter sind nicht eindeutig geprüft oder nicht unterstützt.",
        ));
    }
    Ok(())
}
fn safe_uuid(value: &str) -> bool {
    (value.len() == 9
        && value.as_bytes()[4] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 4 || b.is_ascii_hexdigit()))
        || Uuid::parse_str(value).is_ok() && value.len() == 36
}
fn safe_path(value: &str) -> bool {
    value.len() <= 512
        && value.starts_with('/')
        && value.len() > 1
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/_.-@".contains(&b))
        && value[1..]
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
fn safe_parameter(value: &str) -> bool {
    if matches!(
        value,
        "quiet" | "splash" | "rw" | "ro" | "nowatchdog" | "no_watchdog" | "loglevel=3"
    ) {
        return true;
    }
    if let Some(uuid) = value.strip_prefix("root=UUID=") {
        return uuid.len() == 36 && Uuid::parse_str(uuid).is_ok();
    }
    if let Some(path) = value.strip_prefix("rootflags=subvol=") {
        return safe_path(path);
    }
    if let Some(number) = value.strip_prefix("loglevel=") {
        return number.len() == 1
            && number.as_bytes()[0].is_ascii_digit()
            && number.as_bytes()[0] <= b'7';
    }
    false
}
