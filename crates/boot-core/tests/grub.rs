use boot_core::*;
use chrono::{TimeZone, Utc};
use uuid::Uuid;

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 17, 12, 0, 0).unwrap()
}
fn config(user: &str) -> Vec<u8> {
    include_str!("../../../tests/fixtures/grub-main.cfg")
        .replace(include_str!("../../../tests/fixtures/grub-user.cfg"), user)
        .into_bytes()
}
fn user() -> &'static str {
    include_str!("../../../tests/fixtures/grub-user.cfg")
}
fn snapshot() -> Snapshot {
    let mut s = Snapshot::new(Uuid::new_v4(), now());
    s.sources.grub = CollectionState::Complete;
    s.grub = Some(parse_grub(&config(user()), user().as_bytes()).unwrap());
    s
}
fn menu(s: &Snapshot, id: String) -> ChangePlan {
    prepare_plan(
        s,
        PlanRequest::Menu {
            default_entry_id: id,
            timeout_seconds: 12,
        },
        now(),
    )
    .unwrap()
}
fn bytes(plan: &ChangePlan, target: Target) -> Vec<u8> {
    match &plan
        .changes
        .iter()
        .find(|c| c.target == target)
        .unwrap()
        .replacement
    {
        TargetValue::FileBytes(b) => b.clone(),
        _ => panic!(),
    }
}
#[test]
fn preserves_unmanaged_bytes_and_crlf() {
    let s = snapshot();
    let id = s.grub.as_ref().unwrap().entries[0].id.clone();
    let p = menu(&s, id);
    let main = bytes(&p, Target::GrubMain);
    let expected = String::from_utf8(config(user()))
        .unwrap()
        .replace("set timeout=30", "set timeout=12")
        .replace("set timeout=45", "set timeout=12");
    assert_eq!(main, expected.as_bytes());
    assert_eq!(p.expires_at - p.created_at, chrono::Duration::minutes(15));
    assert_eq!(p.snapshot_id, s.id);
    assert!(
        p.preconditions
            .iter()
            .all(|p| p.expected_checksum.len() == 64)
    );
}
#[test]
fn mismatched_usersection_blocks_write() {
    let d = parse_grub(&config(user()), b"# different\n").unwrap();
    assert!(!d.write_supported);
    assert!(d.write_block_reason.is_some());
}
#[test]
fn dynamic_default_blocks_write() {
    let b = String::from_utf8(config(user()))
        .unwrap()
        .replace("set default=0", "set default=${saved_entry}");
    assert!(
        !parse_grub(b.as_bytes(), user().as_bytes())
            .unwrap()
            .write_supported
    );
}
#[test]
fn multiple_timeout_overrides_are_handled_explicitly() {
    let s = snapshot();
    assert_eq!(s.grub.as_ref().unwrap().timeout_seconds, Some(45));
    let p = menu(&s, "fallback".into());
    let m = bytes(&p, Target::GrubMain);
    let u = bytes(&p, Target::GrubUser);
    let d = parse_grub(&m, &u).unwrap();
    assert!(d.write_supported);
    assert_eq!(d.timeout_seconds, Some(12));
    assert_eq!(d.default_entry_id.as_deref(), Some("fallback"));
    assert!(!String::from_utf8(m).unwrap().contains("timeout=45"));
}
#[test]
fn stable_ids_resolve_complete_flat_menu_index() {
    let s = snapshot();
    let entries = &s.grub.as_ref().unwrap().entries;
    assert_eq!(entries.len(), 4);
    let p = menu(&s, entries[1].id.clone());
    let b = bytes(&p, Target::GrubMain);
    assert!(String::from_utf8_lossy(&b).contains("set default=1"));
    let d = parse_grub(&b, &bytes(&p, Target::GrubUser)).unwrap();
    assert_eq!(d.entries[1].id, entries[1].id);
}
#[test]
fn ambiguous_and_conditional_settings_block_write() {
    for replace in [
        "if true; then set timeout=5; fi",
        "set timeout=$delay",
        "set timeout=1; set timeout=2",
        "submenu 'Nested' { menuentry 'Child' { reboot; } }",
    ] {
        let b = String::from_utf8(config(user()))
            .unwrap()
            .replace("set timeout=30", replace);
        assert!(
            !parse_grub(b.as_bytes(), user().as_bytes())
                .unwrap()
                .write_supported,
            "{replace}"
        );
    }
}
#[test]
fn duplicate_ids_and_markers_block_write() {
    for u in [
        user().replace("--id=fallback", "--id=installer"),
        format!("{}# start-grub2win-user-section ***\r\n", user()),
    ] {
        assert!(
            !parse_grub(&config(&u), u.as_bytes())
                .unwrap()
                .write_supported
        );
    }
}
fn candidate_snapshot() -> (Snapshot, Uuid) {
    let mut s = snapshot();
    s.sources.linux = CollectionState::Complete;
    s.sources.volumes = CollectionState::Complete;
    let id = Uuid::new_v4();
    let volume = Uuid::new_v4();
    s.volumes.push(VolumeInfo {
        volume_id: Some(volume),
        partition_id: None,
        paths: vec![],
        filesystem: Some("FAT32".into()),
        filesystem_uuid: Some("AB12-CD34".into()),
        size_bytes: None,
        disk_number: None,
        partition_number: None,
        gpt_type: None,
    });
    s.linux_candidates.push(LinuxCandidate {
        id,
        title: "CachyOS".into(),
        volume_id: volume,
        filesystem_uuid: "AB12-CD34".into(),
        kernel_path: "/managed/linux-cachyos/vmlinuz".into(),
        initramfs_paths: vec!["/managed/linux-cachyos/initramfs".into()],
        kernel_parameters: vec![
            "quiet".into(),
            "rw".into(),
            format!("root=UUID={}", Uuid::nil()),
            "rootflags=subvol=/@".into(),
        ],
        validated: true,
        evidence: vec![
            EvidenceRef {
                source: Source::Volumes,
                reference: "volumes[0]".into(),
            },
            EvidenceRef {
                source: Source::Linux,
                reference: "checked_images".into(),
            },
            EvidenceRef {
                source: Source::Grub,
                reference: "existing_config".into(),
            },
        ],
    });
    (s, id)
}
#[test]
fn cachyos_uses_verified_managed_image_paths() {
    let (s, id) = candidate_snapshot();
    let p = prepare_plan(
        &s,
        PlanRequest::Cachyos {
            candidate_id: id,
            title: "CachyOS's main".into(),
        },
        now(),
    )
    .unwrap();
    let u = String::from_utf8(bytes(&p, Target::GrubUser)).unwrap();
    assert!(u.starts_with(user()));
    assert!(u.contains("linux /managed/linux-cachyos/vmlinuz quiet rw"));
    assert!(u.contains("initrd /managed/linux-cachyos/initramfs"));
    assert!(u.contains("AB12-CD34"));
    let d = parse_grub(&bytes(&p, Target::GrubMain), u.as_bytes()).unwrap();
    assert!(d.write_supported, "{:?}", d.write_block_reason);
    assert_eq!(d.entries.last().unwrap().title, "CachyOS's main");
}
#[test]
fn kernel_tokens_cannot_inject_grub_code() {
    for bad in [
        "quiet;reboot",
        "root=$(reboot)",
        "init=/bin/sh",
        "quiet\nreboot",
        "foo=bar",
        "rootflags=subvol=/../bad",
    ] {
        let (mut s, id) = candidate_snapshot();
        s.linux_candidates[0].kernel_parameters.push(bad.into());
        assert!(
            prepare_plan(
                &s,
                PlanRequest::Cachyos {
                    candidate_id: id,
                    title: "Linux".into()
                },
                now()
            )
            .is_err(),
            "{bad}"
        );
    }
}
#[test]
fn rejects_unverified_or_unsafe_paths() {
    for bad in [
        "/../vmlinuz",
        "/boot/a;reboot",
        "relative",
        "/boot//vmlinuz",
        "/boot/$kernel",
        "/boot/vmlinuz\n",
    ] {
        let (mut s, id) = candidate_snapshot();
        s.linux_candidates[0].kernel_path = bad.into();
        assert!(
            prepare_plan(
                &s,
                PlanRequest::Cachyos {
                    candidate_id: id,
                    title: "Linux".into()
                },
                now()
            )
            .is_err()
        );
    }
    let (mut s, id) = candidate_snapshot();
    s.volumes.clear();
    assert!(
        prepare_plan(
            &s,
            PlanRequest::Cachyos {
                candidate_id: id,
                title: "Linux".into()
            },
            now()
        )
        .is_err()
    );
}
#[test]
fn order_plan_retains_all_entries() {
    let mut s = snapshot();
    s.sources.firmware = CollectionState::Complete;
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    s.firmware_entries = vec![
        FirmwareEntry {
            id: a,
            title: "Windows".into(),
            loader_path: None,
        },
        FirmwareEntry {
            id: b,
            title: "Linux".into(),
            loader_path: None,
        },
    ];
    s.firmware_order = vec![a, b];
    assert!(prepare_plan(&s, PlanRequest::Order { entry_ids: vec![b] }, now()).is_err());
    let p = prepare_plan(
        &s,
        PlanRequest::Order {
            entry_ids: vec![b, a],
        },
        now(),
    )
    .unwrap();
    assert_eq!(
        p.changes[0].expected,
        TargetValue::FirmwareOrder(vec![a, b])
    );
    assert_eq!(
        p.changes[0].replacement,
        TargetValue::FirmwareOrder(vec![b, a])
    );
}
#[test]
fn reparse_prevents_forged_document_metadata() {
    let mut s = snapshot();
    s.grub.as_mut().unwrap().main_bytes = String::from_utf8(config(user()))
        .unwrap()
        .replace("set default=0", "set default=$saved_entry")
        .into_bytes();
    assert!(
        prepare_plan(
            &s,
            PlanRequest::Menu {
                default_entry_id: "fallback".into(),
                timeout_seconds: 12
            },
            now()
        )
        .is_err()
    );
}
#[test]
fn unrecognized_sources_and_late_dynamic_overrides_are_read_only() {
    for source in [
        "source /other.cfg",
        "eval 'set timeout=0'",
        "load_env",
        "set timeout=$external",
    ] {
        let original = String::from_utf8(config(user())).unwrap();
        let main = format!("{original}{source}\r\n");
        assert!(
            !parse_grub(main.as_bytes(), user().as_bytes())
                .unwrap()
                .write_supported,
            "{source}"
        );
    }
}
#[test]
fn exact_grub2win_preamble_is_supported_but_not_arbitrary_variants() {
    let preamble = "source $prefix/g2bootmgr/gnugrub.functions.cfg\r\ng2wbits\r\nset gfxmode=1024x768,auto\r\nif [ $grub2win_bootmode = EFI ] ; then set theme=$theme.$grub2win_efibits.efi.txt ; else set theme=$theme.$grub2win_procbits.bios.txt ; fi\r\n";
    let main = String::from_utf8(config(user())).unwrap().replace(
        "menuentry 'Windows'",
        &format!("{preamble}menuentry 'Windows'"),
    );
    assert!(
        parse_grub(main.as_bytes(), user().as_bytes())
            .unwrap()
            .write_supported
    );
    let changed = main.replace("g2wbits\r\n", "g2wbits; set timeout=0\r\n");
    assert!(
        !parse_grub(changed.as_bytes(), user().as_bytes())
            .unwrap()
            .write_supported
    );
}
#[test]
fn checksum_is_domain_separated_and_sensitive_to_exact_bytes_and_order() {
    let a = Uuid::from_u128(1);
    let b = Uuid::from_u128(2);
    assert_ne!(
        target_checksum(&TargetValue::FirmwareOrder(vec![a, b])),
        target_checksum(&TargetValue::FirmwareOrder(vec![b, a]))
    );
    assert_ne!(
        target_checksum(&TargetValue::FileBytes(b"x\n".to_vec())),
        target_checksum(&TargetValue::FileBytes(b"x\r\n".to_vec()))
    );
    assert_ne!(
        target_checksum(&TargetValue::FileBytes(vec![])),
        target_checksum(&TargetValue::FirmwareOrder(vec![]))
    );
}
#[test]
fn malformed_syntax_and_invalid_utf8_remain_read_only() {
    for main in [
        b"\xff".to_vec(),
        config(user()).into_iter().take(110).collect(),
        String::from_utf8(config(user()))
            .unwrap()
            .replace("menuentry 'Windows'", "menuentry 'Windows")
            .into_bytes(),
    ] {
        let doc = parse_grub(&main, user().as_bytes()).unwrap();
        assert!(!doc.write_supported);
        assert_eq!(doc.main_bytes, main);
    }
}
#[test]
fn partial_linux_inventory_accepts_only_individually_verified_candidate() {
    let (mut s, id) = candidate_snapshot();
    s.sources.linux = CollectionState::Partial;
    s.linux_candidates[0]
        .evidence
        .retain(|e| e.source != Source::Grub);
    let request = PlanRequest::Cachyos {
        candidate_id: id,
        title: "CachyOS".into(),
    };
    assert!(validate_request(&request, &s).is_ok());
    assert!(prepare_plan(&s, request.clone(), now()).is_ok());
    s.linux_candidates[0].validated = false;
    assert!(validate_request(&request, &s).is_err());
    s.linux_candidates[0].validated = true;
    s.linux_candidates[0].evidence.clear();
    assert!(validate_request(&request, &s).is_err());
    s.linux_candidates.clear();
    assert!(validate_request(&request, &s).is_err());
    for state in [CollectionState::Unknown, CollectionState::Unavailable] {
        let (mut s, id) = candidate_snapshot();
        s.sources.linux = state;
        assert!(
            validate_request(
                &PlanRequest::Cachyos {
                    candidate_id: id,
                    title: "CachyOS".into()
                },
                &s
            )
            .is_err()
        );
    }
}
#[test]
fn generated_fixture_outputs_round_trip() {
    let (s, id) = candidate_snapshot();
    let plans = [
        ("menu", menu(&s, "fallback".into())),
        (
            "cachyos",
            prepare_plan(
                &s,
                PlanRequest::Cachyos {
                    candidate_id: id,
                    title: "CachyOS's $kernel; literal title".into(),
                },
                now(),
            )
            .unwrap(),
        ),
    ];
    for (name, plan) in plans {
        let main = bytes(&plan, Target::GrubMain);
        let user = bytes(&plan, Target::GrubUser);
        assert!(parse_grub(&main, &user).unwrap().write_supported);
        // Explicit opt-in creates synthetic artifacts for the independent GRUB checker.
        if let Some(directory) = std::env::var_os("BOOTPILOT_GRUB_FIXTURE_OUTPUT") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join(format!("{name}-main.cfg")), main).unwrap();
            std::fs::write(directory.join(format!("{name}-user.cfg")), user).unwrap();
        }
    }
}
#[test]
fn marker_boundaries_cannot_cut_through_menu_blocks() {
    let main = "set default=0\nset timeout=30\nmenuentry 'Outer' {\n# start-grub2win-user-section ***\necho inner\n# end-grub2win-user-section ***\n}\n";
    assert!(
        !parse_grub(main.as_bytes(), b"echo inner\n")
            .unwrap()
            .write_supported
    );
}
#[test]
fn numeric_explicit_ids_cannot_misroute_default_selection() {
    let u = user().replace("--id=fallback", "--id=0");
    assert!(
        !parse_grub(&config(&u), u.as_bytes())
            .unwrap()
            .write_supported
    );
}
#[test]
fn all_literal_default_overrides_are_changed_and_entry_bodies_are_untouched() {
    let u = format!("set default=installer\r\n{}", user());
    let mut s = snapshot();
    s.grub = Some(parse_grub(&config(&u), u.as_bytes()).unwrap());
    let p = menu(&s, "fallback".into());
    let main = String::from_utf8(bytes(&p, Target::GrubMain)).unwrap();
    assert_eq!(main.matches("set default=fallback").count(), 2);
    assert!(main.contains("chainloader /EFI/Microsoft/Boot/bootmgfw.efi"));
    assert!(main.contains("chainloader /EFI/old.efi"));
}
#[test]
fn preamble_cannot_redirect_trusted_source_or_load_unknown_modules() {
    for command in [
        "set prefix=/unverified",
        "insmod arbitrary_module",
        "terminal_output $unknown",
        "loadfont /unverified/font.pf2",
        "set timeout_style=$unknown",
    ] {
        let original = String::from_utf8(config(user())).unwrap();
        let main = original.replace("set default=0", &format!("{command}\r\nset default=0"));
        assert!(
            !parse_grub(main.as_bytes(), user().as_bytes())
                .unwrap()
                .write_supported,
            "{command}"
        );
    }
}
