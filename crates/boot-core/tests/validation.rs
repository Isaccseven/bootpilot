use boot_core::*;
use chrono::{TimeZone, Utc};
use uuid::Uuid;

fn snapshot() -> Snapshot {
    let mut snapshot = Snapshot::new(
        Uuid::from_u128(1),
        Utc.with_ymd_and_hms(2026, 10, 17, 10, 0, 0).unwrap(),
    );
    snapshot.sources.firmware = CollectionState::Complete;
    snapshot.firmware_entries = vec![
        FirmwareEntry {
            id: Uuid::from_u128(10),
            title: "Windows".into(),
            loader_path: None,
        },
        FirmwareEntry {
            id: Uuid::from_u128(20),
            title: "GRUB".into(),
            loader_path: None,
        },
    ];
    snapshot.firmware_order = vec![Uuid::from_u128(10), Uuid::from_u128(20)];
    snapshot.sources.grub = CollectionState::Complete;
    snapshot.grub = Some(GrubDocument {
        main_bytes: Vec::new(),
        user_bytes: Vec::new(),
        entries: vec![MenuEntry {
            id: "windows".into(),
            title: "Windows".into(),
            kind: MenuEntryKind::Windows,
        }],
        default_entry_id: Some("windows".into()),
        timeout_seconds: Some(10),
        write_supported: true,
        write_block_reason: None,
    });
    snapshot
}

#[test]
fn order_must_be_permutation() {
    let snapshot = snapshot();
    let a = Uuid::from_u128(10);
    let b = Uuid::from_u128(20);
    assert!(
        validate_request(
            &PlanRequest::Order {
                entry_ids: vec![b, a]
            },
            &snapshot
        )
        .is_ok()
    );
    for ids in [vec![a, a], vec![a], vec![a, Uuid::from_u128(30)], vec![]] {
        assert!(validate_request(&PlanRequest::Order { entry_ids: ids }, &snapshot).is_err());
    }
}

#[test]
fn menu_requires_known_entry_and_timeout() {
    let snapshot = snapshot();
    for timeout in [1, 300] {
        assert!(
            validate_request(
                &PlanRequest::Menu {
                    default_entry_id: "windows".into(),
                    timeout_seconds: timeout
                },
                &snapshot
            )
            .is_ok()
        );
    }
    for (id, timeout) in [
        ("missing", 10),
        ("windows", 0),
        ("windows", 301),
        ("windows\n", 10),
    ] {
        assert!(
            validate_request(
                &PlanRequest::Menu {
                    default_entry_id: id.into(),
                    timeout_seconds: timeout
                },
                &snapshot
            )
            .is_err()
        );
    }
}

#[test]
fn unknown_security_state_stays_unknown() {
    let snapshot = Snapshot::new(Uuid::from_u128(1), Utc::now());
    let json = serde_json::to_value(&snapshot).unwrap();
    assert!(json["secure_boot"].is_null());
    assert!(json["bitlocker"].is_null());
    assert_eq!(json["sources"]["secure_boot"], "unknown");
    let restored: Snapshot = serde_json::from_value(json).unwrap();
    assert_eq!(restored.secure_boot, None);
    assert_eq!(restored.bitlocker, None);
}

#[test]
fn diagnosis_distinguishes_evidence_from_hypothesis() {
    let findings = diagnose(&snapshot());
    let unknown = findings
        .iter()
        .find(|finding| finding.code == "boot_current_unknown")
        .unwrap();
    assert_eq!(unknown.certainty, FindingCertainty::Evidence);
    assert!(!unknown.evidence.is_empty());
    assert!(unknown.open_check.is_some());
    assert!(
        !findings
            .iter()
            .any(|finding| finding.statement.contains("übersprungen"))
    );
}

#[test]
fn incomplete_inventory_cannot_authorize_changes() {
    let mut snapshot = snapshot();
    snapshot.sources.firmware = CollectionState::Partial;
    assert_eq!(
        validate_request(
            &PlanRequest::Order {
                entry_ids: snapshot.firmware_order.clone()
            },
            &snapshot
        )
        .unwrap_err()
        .code,
        ErrorCode::IncompleteSnapshot
    );
    snapshot.sources.grub = CollectionState::Unavailable;
    assert!(
        validate_request(
            &PlanRequest::Menu {
                default_entry_id: "windows".into(),
                timeout_seconds: 5
            },
            &snapshot
        )
        .is_err()
    );
}

#[test]
fn ambiguous_or_unsupported_menu_cannot_authorize_changes() {
    let mut snapshot = snapshot();
    let request = PlanRequest::Menu {
        default_entry_id: "windows".into(),
        timeout_seconds: 5,
    };
    snapshot.grub.as_mut().unwrap().write_supported = false;
    assert!(validate_request(&request, &snapshot).is_err());
    let grub = snapshot.grub.as_mut().unwrap();
    grub.write_supported = true;
    grub.entries.push(grub.entries[0].clone());
    assert!(validate_request(&request, &snapshot).is_err());
}

#[test]
fn only_verified_linux_candidates_and_plain_titles_are_accepted() {
    let mut snapshot = snapshot();
    snapshot.sources.linux = CollectionState::Complete;
    snapshot.sources.volumes = CollectionState::Complete;
    snapshot.volumes.push(VolumeInfo {
        volume_id: Some(Uuid::from_u128(50)),
        partition_id: None,
        paths: vec![],
        filesystem: Some("FAT32".into()),
        filesystem_uuid: Some("AB12-CD34".into()),
        size_bytes: None,
        disk_number: None,
        partition_number: None,
        gpt_type: None,
    });
    let candidate = LinuxCandidate {
        id: Uuid::from_u128(40),
        title: "CachyOS".into(),
        volume_id: Uuid::from_u128(50),
        filesystem_uuid: "AB12-CD34".into(),
        kernel_path: "/boot/vmlinuz-linux-cachyos".into(),
        initramfs_paths: vec!["/boot/initramfs-linux-cachyos.img".into()],
        kernel_parameters: vec![
            "rw".into(),
            "root=UUID=11111111-2222-3333-4444-555555555555".into(),
        ],
        validated: true,
        evidence: vec![
            EvidenceRef {
                source: Source::Volumes,
                reference: "volumes[0]".into(),
            },
            EvidenceRef {
                source: Source::Linux,
                reference: "verified_config_and_images".into(),
            },
        ],
    };
    snapshot.linux_candidates.push(candidate);
    let request = PlanRequest::Cachyos {
        candidate_id: Uuid::from_u128(40),
        title: "CachyOS".into(),
    };
    assert!(validate_request(&request, &snapshot).is_ok());
    snapshot.linux_candidates[0].validated = false;
    assert!(validate_request(&request, &snapshot).is_err());
    snapshot.linux_candidates[0].validated = true;
    for title in ["", "   ", "CachyOS\nreboot", "CachyOS\0"] {
        assert!(
            validate_request(
                &PlanRequest::Cachyos {
                    candidate_id: Uuid::from_u128(40),
                    title: title.into()
                },
                &snapshot
            )
            .is_err()
        );
    }
    assert!(
        validate_request(
            &PlanRequest::Cachyos {
                candidate_id: Uuid::from_u128(99),
                title: "CachyOS".into()
            },
            &snapshot
        )
        .is_err()
    );
}

#[test]
fn request_ids_are_typed_and_normalized() {
    let request: PlanRequest = serde_json::from_str(
        r#"{"kind":"order","entry_ids":["{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}"]}"#,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(request).unwrap()["entry_ids"][0],
        "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
    );
    assert!(
        serde_json::from_str::<PlanRequest>(r#"{"kind":"restore","backup_id":"../../other"}"#)
            .is_err()
    );
    assert!(serde_json::from_str::<PlanRequest>(r#"{"kind":"menu","default_entry_id":"windows","timeout_seconds":2,"path":"C:/arbitrary"}"#).is_err());
}

#[test]
fn structured_collection_failures_are_retained_in_fixture() {
    let snapshot: Snapshot = serde_json::from_str(include_str!(
        "../../../tests/fixtures/unknown-snapshot.json"
    ))
    .unwrap();
    assert_eq!(snapshot.sources.firmware, CollectionState::Unavailable);
    assert_eq!(
        snapshot.source_errors[0].error.code,
        ErrorCode::AccessDenied
    );
    assert_eq!(snapshot.secure_boot, None);
    assert!(
        diagnose(&snapshot)
            .iter()
            .any(|finding| finding.code == "source_error")
    );
}

#[test]
fn malformed_complete_inventory_is_rejected() {
    let mut snapshot = snapshot();
    snapshot
        .firmware_entries
        .push(snapshot.firmware_entries[0].clone());
    assert!(
        validate_request(
            &PlanRequest::Order {
                entry_ids: snapshot.firmware_order.clone()
            },
            &snapshot
        )
        .is_err()
    );
    snapshot.firmware_entries.pop();
    snapshot.firmware_order.pop();
    assert!(
        validate_request(
            &PlanRequest::Order {
                entry_ids: vec![Uuid::from_u128(10), Uuid::from_u128(20)]
            },
            &snapshot
        )
        .is_err()
    );
}

#[test]
fn nil_restore_ids_and_unknown_schema_are_rejected() {
    let mut snapshot = snapshot();
    assert!(
        validate_request(
            &PlanRequest::Restore {
                backup_id: Uuid::nil()
            },
            &snapshot
        )
        .is_err()
    );
    assert!(
        validate_request(
            &PlanRequest::Restore {
                backup_id: Uuid::from_u128(1)
            },
            &snapshot
        )
        .is_ok()
    );
    snapshot.schema_version = 2;
    assert!(
        validate_request(
            &PlanRequest::Restore {
                backup_id: Uuid::from_u128(1)
            },
            &snapshot
        )
        .is_err()
    );
}
