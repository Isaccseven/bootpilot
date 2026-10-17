use boot_core::*;
use boot_windows::snapshot::{InventorySource, collect_with};
struct Denied;
impl InventorySource for Denied {
    fn collect(&mut self, _: Source, _: &mut Snapshot) -> BootResult<()> {
        Err(BootError::new(
            ErrorCode::AccessDenied,
            "Zugriff verweigert",
        ))
    }
}
#[test]
fn access_denied_is_incomplete_inventory() {
    let snapshot = collect_with(&mut Denied);
    assert_eq!(snapshot.sources.bcd, CollectionState::Unavailable);
    assert_eq!(snapshot.sources.linux, CollectionState::Unavailable);
    assert_eq!(snapshot.bitlocker, None);
    assert_eq!(snapshot.source_errors.len(), 8);
}

#[test]
fn one_denied_source_does_not_discard_others() {
    struct Mixed;
    impl InventorySource for Mixed {
        fn collect(&mut self, source: Source, snapshot: &mut Snapshot) -> BootResult<()> {
            if source == Source::Bitlocker {
                Err(BootError::new(ErrorCode::AccessDenied, "verweigert"))
            } else {
                if source == Source::SecureBoot {
                    snapshot.secure_boot = Some(true)
                }
                Ok(())
            }
        }
    }
    let snapshot = collect_with(&mut Mixed);
    assert_eq!(snapshot.secure_boot, Some(true));
    assert_eq!(snapshot.sources.secure_boot, CollectionState::Complete);
    assert_eq!(snapshot.sources.bitlocker, CollectionState::Unavailable);
    assert_eq!(snapshot.bitlocker, None);
}
#[test]
fn bcd_boolean_and_bitlocker_numeric_returns_are_distinct() {
    use boot_windows::bcd::*;
    assert!(bcd_return_is_success(&serde_json::json!(true)));
    assert!(!bcd_return_is_success(&serde_json::json!(0)));
    assert!(!bcd_return_is_success(&serde_json::json!(false)));
    assert!(bitlocker_return_is_success(&serde_json::json!(0)));
    assert!(!bitlocker_return_is_success(&serde_json::json!(true)));
    assert!(!bitlocker_return_is_success(&serde_json::json!(1)));
}
#[test]
fn malformed_efi_options_are_rejected() {
    use boot_windows::firmware::*;
    assert!(parse_load_option(&[0; 7]).is_err());
    assert!(parse_load_option(&[0, 0, 0, 0, 4, 0, 65, 0]).is_err());
    assert!(parse_load_option(&[0, 0, 0, 0, 4, 0, 0, 0, 4, 4, 3, 0]).is_err());
    let valid = [0, 0, 0, 0, 4, 0, 65, 0, 0, 0, 127, 255, 4, 0];
    assert_eq!(parse_load_option(&valid).unwrap().description, "A");
    assert!(parse_u16(&[0]).is_err());
    assert_eq!(parse_u16(&[3, 1]).unwrap(), 259);
}
#[test]
fn partition_guid_uses_efi_mixed_endian_layout() {
    let mut bytes = vec![0, 0, 0, 0, 46, 0, 0, 0];
    let mut hd = vec![0u8; 42];
    hd[..4].copy_from_slice(&[4, 1, 42, 0]);
    hd[24..40].copy_from_slice(&[
        0x33, 0x22, 0x11, 0, 0x55, 0x44, 0x77, 0x66, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
    ]);
    hd[41] = 2;
    bytes.extend(hd);
    bytes.extend([127, 255, 4, 0]);
    assert_eq!(
        boot_windows::firmware::parse_load_option(&bytes)
            .unwrap()
            .partition_id
            .unwrap()
            .to_string(),
        "00112233-4455-6677-8899-aabbccddeeff"
    );
}
#[test]
fn volume_guid_and_snapshot_unknown_fields_are_explicit() {
    assert_eq!(
        boot_windows::volumes::volume_id(r"\\?\Volume{00112233-4455-6677-8899-aabbccddeeff}\")
            .unwrap()
            .to_string(),
        "00112233-4455-6677-8899-aabbccddeeff"
    );
    assert!(boot_windows::volumes::volume_id(r"C:\").is_none());
    let snapshot = Snapshot::new(uuid::Uuid::new_v4(), chrono::Utc::now());
    let value = serde_json::to_value(&snapshot).unwrap();
    assert!(value["boot_next"].is_null());
    assert!(value["loader_info"].is_null());
    assert_eq!(value["volumes"], serde_json::json!([]));
    assert_eq!(serde_json::from_value::<Snapshot>(value).unwrap(), snapshot);
}
#[test]
fn native_write_adapter_is_closed() {
    let mut system = boot_windows::WindowsSystem;
    let value = TargetValue::FirmwareOrder(Vec::new());
    assert_eq!(
        system
            .write_target(&Target::FirmwareOrder, &value, &value)
            .unwrap_err()
            .code,
        ErrorCode::NotImplemented
    );
}

#[test]
fn existing_conditional_config_has_stable_candidate_identity() {
    use boot_windows::volumes::*;
    let config=b"menuentry 'Installed Linux' {\n if search --fs-uuid --set=root A123-B456; then\n linux /boot/vmlinuz quiet root=UUID=00112233-4455-6677-8899-aabbccddeeff\n initrd /boot/initramfs\n fi\n}\n";
    let references = linux_references(config);
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].filesystem_uuid, "A123-B456");
    assert_eq!(references[0].title, "Installed Linux");
    let id = candidate_id(uuid::Uuid::from_u128(1), &references[0]);
    assert_eq!(
        id,
        candidate_id(uuid::Uuid::from_u128(1), &linux_references(config)[0])
    );
    assert_ne!(id, candidate_id(uuid::Uuid::from_u128(2), &references[0]));
    let mut changed = references[0].clone();
    changed.kernel_path = "/boot/other".into();
    assert_ne!(id, candidate_id(uuid::Uuid::from_u128(1), &changed));
}

#[test]
fn older_snapshots_get_unknown_additive_fields() {
    let snapshot = Snapshot::new(uuid::Uuid::new_v4(), chrono::Utc::now());
    let mut value = serde_json::to_value(snapshot).unwrap();
    for key in [
        "boot_next",
        "loader_info",
        "loader_entry_selected",
        "volumes",
    ] {
        value.as_object_mut().unwrap().remove(key);
    }
    value["sources"].as_object_mut().unwrap().remove("volumes");
    let restored: Snapshot = serde_json::from_value(value).unwrap();
    assert_eq!(restored.boot_next, None);
    assert_eq!(restored.sources.volumes, CollectionState::Unknown);
}
