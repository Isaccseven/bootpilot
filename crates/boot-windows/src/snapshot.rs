use boot_core::*;
use chrono::Utc;
use uuid::Uuid;

/// An adapter collects one source. Failed sources never become complete by default.
pub trait InventorySource {
    fn collect(&mut self, source: Source, snapshot: &mut Snapshot) -> BootResult<()>;
}
pub fn collect_with(adapter: &mut impl InventorySource) -> Snapshot {
    let mut snapshot = Snapshot::new(Uuid::new_v4(), Utc::now());
    for source in [
        Source::Volumes,
        Source::Bcd,
        Source::Firmware,
        Source::BootCurrent,
        Source::SecureBoot,
        Source::Bitlocker,
        Source::Grub,
        Source::Linux,
    ] {
        let result = adapter.collect(source, &mut snapshot);
        let state = match result {
            Ok(()) => CollectionState::Complete,
            Err(error) => {
                let state = if error.code == ErrorCode::IncompleteSnapshot {
                    CollectionState::Partial
                } else {
                    CollectionState::Unavailable
                };
                snapshot.source_errors.push(SourceError { source, error });
                state
            }
        };
        *match source {
            Source::Volumes => &mut snapshot.sources.volumes,
            Source::Bcd => &mut snapshot.sources.bcd,
            Source::Firmware => &mut snapshot.sources.firmware,
            Source::BootCurrent => &mut snapshot.sources.boot_current,
            Source::SecureBoot => &mut snapshot.sources.secure_boot,
            Source::Bitlocker => &mut snapshot.sources.bitlocker,
            Source::Grub => &mut snapshot.sources.grub,
            Source::Linux => &mut snapshot.sources.linux,
        } = state;
    }
    snapshot
}
pub fn collect_snapshot() -> BootResult<Snapshot> {
    #[cfg(windows)]
    {
        Ok(collect_with(&mut native_inventory::Inventory))
    }
    #[cfg(not(windows))]
    {
        Err(super::error(
            ErrorCode::NotImplemented,
            "Windows erforderlich",
        ))
    }
}
#[derive(Default)]
pub struct WindowsSystem;
impl SystemAccess for WindowsSystem {
    fn read_snapshot(&mut self) -> BootResult<Snapshot> {
        collect_snapshot()
    }
    fn read_target(&mut self, target: &Target) -> BootResult<TargetValue> {
        let snapshot = collect_snapshot()?;
        match target {
            Target::FirmwareOrder if snapshot.sources.bcd == CollectionState::Complete => {
                Ok(TargetValue::FirmwareOrder(snapshot.firmware_order))
            }
            Target::GrubMain | Target::GrubUser => snapshot
                .grub
                .map(|g| {
                    TargetValue::FileBytes(if *target == Target::GrubMain {
                        g.main_bytes
                    } else {
                        g.user_bytes
                    })
                })
                .ok_or_else(|| {
                    super::error(
                        ErrorCode::IncompleteSnapshot,
                        "GRUB-Datei nicht vollständig erfasst",
                    )
                }),
            _ => Err(super::error(
                ErrorCode::IncompleteSnapshot,
                "Bootquelle nicht vollständig erfasst",
            )),
        }
    }
    fn write_target(&mut self, _: &Target, _: &TargetValue, _: &TargetValue) -> BootResult<()> {
        Err(super::error(
            ErrorCode::NotImplemented,
            "Boot-Schreiben wird erst durch den Transaktionsdienst freigegeben",
        ))
    }
}

#[cfg(windows)]
mod native_inventory {
    use super::*;
    use crate::native::*;
    use std::path::PathBuf;
    use windows::Win32::System::SystemInformation::*;
    const GLOBAL: &str = "{8be4df61-93ca-11d2-aa0d-00e098032b8c}";
    const LOADER: &str = "{4a67b082-0a4c-41cf-b6c7-440b29bb8c4f}";
    pub(crate) struct Inventory;
    impl crate::snapshot::InventorySource for Inventory {
        fn collect(&mut self, source: Source, snapshot: &mut Snapshot) -> BootResult<()> {
            match source {
                Source::Volumes => crate::volumes::collect(snapshot),
                Source::Bcd => crate::bcd::collect(snapshot),
                Source::Firmware => {
                    let mut kind = FIRMWARE_TYPE::default();
                    unsafe {
                        GetFirmwareType(&mut kind).map_err(win_error)?;
                    }
                    if kind != FirmwareTypeUefi {
                        return Err(crate::error(
                            ErrorCode::UnsupportedConfiguration,
                            "System läuft nicht im UEFI-Modus",
                        ));
                    }
                    let mut failures = Vec::new();
                    match variable("BootNext", GLOBAL) {
                        Ok(v) => snapshot.boot_next = Some(crate::firmware::parse_u16(&v)?),
                        Err(e) if e.code == ErrorCode::NotFound => {}
                        Err(e) => failures.push(e),
                    }
                    for name in ["LoaderInfo", "LoaderEntrySelected"] {
                        match variable(name, LOADER) {
                            Ok(v) => {
                                let v = Some(crate::firmware::parse_utf16(&v)?);
                                if name == "LoaderInfo" {
                                    snapshot.loader_info = v
                                } else {
                                    snapshot.loader_entry_selected = v
                                }
                            }
                            Err(e) if e.code == ErrorCode::NotFound => {}
                            Err(e) => failures.push(e),
                        }
                    }
                    if failures.is_empty() {
                        Ok(())
                    } else {
                        Err(crate::error(
                            ErrorCode::IncompleteSnapshot,
                            "UEFI-Zusatzdaten teilweise nicht lesbar",
                        )
                        .with_details(serde_json::json!({"errors":failures})))
                    }
                }
                Source::BootCurrent => {
                    let number = crate::firmware::parse_u16(&variable("BootCurrent", GLOBAL)?)?;
                    snapshot.boot_current = Some(BootCurrent {
                        uefi_number: number,
                        description: None,
                        bcd_id: None,
                    });
                    let option = crate::firmware::parse_load_option(&variable(
                        &format!("Boot{number:04X}"),
                        GLOBAL,
                    )?)?;
                    snapshot.boot_current.as_mut().unwrap().description = Some(option.description);
                    // No correlation by description or list position: BCD has not established partition identity.
                    Ok(())
                }
                Source::SecureBoot => {
                    let b = variable("SecureBoot", GLOBAL)?;
                    snapshot.secure_boot = match b.as_slice() {
                        [0] => Some(false),
                        [1] => Some(true),
                        _ => {
                            return Err(crate::error(
                                ErrorCode::VerificationFailed,
                                "SecureBoot hat einen ungültigen Wert",
                            ));
                        }
                    };
                    Ok(())
                }
                Source::Bitlocker => bitlocker(snapshot),
                Source::Grub => grub(snapshot),
                Source::Linux => linux(snapshot),
            }
        }
    }
    fn bitlocker(snapshot: &mut Snapshot) -> BootResult<()> {
        use serde::Deserialize;
        use wmi::{AuthLevel, Variant, WMIConnection};
        #[derive(Deserialize)]
        struct Os {
            #[serde(rename = "SystemDrive")]
            drive: String,
        }
        #[derive(Deserialize)]
        struct Volume {
            #[serde(rename = "__PATH")]
            path: String,
            #[serde(rename = "DriveLetter")]
            letter: Option<String>,
        }
        let sys = WMIConnection::new().map_err(wmi_error)?;
        let os: Vec<Os> = sys
            .raw_query("SELECT SystemDrive FROM Win32_OperatingSystem")
            .map_err(wmi_error)?;
        let drive = &os
            .first()
            .ok_or_else(|| crate::error(ErrorCode::NotFound, "Windows-Systemlaufwerk unbekannt"))?
            .drive;
        let con =
            WMIConnection::with_namespace_path("ROOT\\CIMV2\\Security\\MicrosoftVolumeEncryption")
                .map_err(wmi_error)?;
        con.set_proxy_blanket(AuthLevel::PktPrivacy)
            .map_err(wmi_error)?;
        let volumes: Vec<Volume> = con
            .raw_query("SELECT __PATH,DriveLetter FROM Win32_EncryptableVolume")
            .map_err(wmi_error)?;
        let volume = volumes
            .iter()
            .find(|v| {
                v.letter
                    .as_ref()
                    .is_some_and(|v| v.eq_ignore_ascii_case(drive))
            })
            .ok_or_else(|| {
                crate::error(
                    ErrorCode::NotFound,
                    "BitLocker-Status des Systemlaufwerks fehlt",
                )
            })?;
        let output = crate::bcd::call(
            &con,
            "Win32_EncryptableVolume",
            &volume.path,
            "GetProtectionStatus",
            &[],
        )?;
        if output.get_property("ReturnValue").map_err(wmi_error)? != Variant::UI4(0) {
            return Err(crate::error(
                ErrorCode::VerificationFailed,
                "BitLocker-Abfrage fehlgeschlagen",
            ));
        }
        snapshot.bitlocker = match output.get_property("ProtectionStatus").map_err(wmi_error)? {
            Variant::UI4(0) => Some(false),
            Variant::UI4(1) => Some(true),
            _ => {
                return Err(crate::error(
                    ErrorCode::IncompleteSnapshot,
                    "BitLocker-Schutzstatus unbekannt",
                ));
            }
        };
        Ok(())
    }
    fn grub(snapshot: &mut Snapshot) -> BootResult<()> {
        let mut found = Vec::new();
        let mut failures = Vec::new();
        for volume in &snapshot.volumes {
            for root in volume
                .paths
                .iter()
                .filter(|p| crate::volumes::volume_id(p).is_some())
            {
                let main = PathBuf::from(root).join("grub2").join("grub.cfg");
                match std::fs::symlink_metadata(&main) {
                    Ok(_) => found.push(main),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => failures.push(crate::io_error(e)),
                }
            }
        }
        found.sort();
        found.dedup();
        if found.len() != 1 {
            return Err(crate::error(
                if found.len() > 1 {
                    ErrorCode::UnsupportedConfiguration
                } else {
                    ErrorCode::IncompleteSnapshot
                },
                "Keine eindeutig lesbare Grub2Win-Konfiguration gefunden",
            )
            .with_details(serde_json::json!({"found":found,"errors":failures})));
        }
        let main = found.pop().unwrap();
        let user = main
            .parent()
            .unwrap()
            .join("userfiles")
            .join("usersection.cfg");
        let main_bytes = read_verified(&main, 512 * 1024)?;
        let user_bytes = read_verified(&user, 512 * 1024)?;
        snapshot.grub_location = Some(GrubLocation {
            main_path: main.to_string_lossy().into(),
            user_path: user.to_string_lossy().into(),
        });
        snapshot.grub = Some(GrubDocument {
            main_bytes,
            user_bytes,
            entries: Vec::new(),
            default_entry_id: None,
            timeout_seconds: None,
            write_supported: false,
            write_block_reason: Some(
                "Rohdaten erfasst; Parserprüfung noch nicht angeschlossen".into(),
            ),
        });
        Ok(())
    }
    fn linux(snapshot: &mut Snapshot) -> BootResult<()> {
        let grub = snapshot.grub.as_ref().ok_or_else(|| {
            crate::error(
                ErrorCode::IncompleteSnapshot,
                "GRUB als Linux-Quelle nicht lesbar",
            )
        })?;
        let references = crate::volumes::linux_references(&grub.user_bytes);
        let mut errors = Vec::new();
        for reference in &references {
            let matched: Vec<_> = snapshot
                .volumes
                .iter()
                .filter(|v| {
                    v.filesystem_uuid
                        .as_ref()
                        .is_some_and(|u| u.eq_ignore_ascii_case(&reference.filesystem_uuid))
                })
                .collect();
            let checked = (|| -> BootResult<LinuxCandidate> {
                if matched.len() != 1 {
                    return Err(crate::error(
                        ErrorCode::IncompleteSnapshot,
                        "Referenziertes Dateisystem ist nicht eindeutig zugänglich",
                    ));
                }
                let volume = matched[0];
                let volume_id = volume.volume_id.ok_or_else(|| {
                    crate::error(ErrorCode::IncompleteSnapshot, "Volume-GUID fehlt")
                })?;
                let root = volume
                    .paths
                    .iter()
                    .find(|p| crate::volumes::volume_id(p).is_some())
                    .ok_or_else(|| {
                        crate::error(
                            ErrorCode::IncompleteSnapshot,
                            "Mountfreier Volume-Pfad fehlt",
                        )
                    })?;
                if reference.initramfs_paths.is_empty() {
                    return Err(crate::error(ErrorCode::InvalidCandidate, "Initramfs fehlt"));
                }
                for relative in
                    std::iter::once(&reference.kernel_path).chain(reference.initramfs_paths.iter())
                {
                    if !relative.starts_with('/')
                        || relative.contains(['$', '(', ')', ':', '\\'])
                        || relative.split('/').any(|s| s == ".." || s == ".")
                    {
                        return Err(crate::error(
                            ErrorCode::UnsupportedConfiguration,
                            "Nur literale absolute Image-Pfade werden geprüft",
                        ));
                    }
                    let path = PathBuf::from(root)
                        .join(relative.trim_start_matches('/').replace('/', "\\"));
                    let check = (|| -> BootResult<()> {
                        let _pins = pin_path(&path)?;
                        let file = std::fs::File::open(&path).map_err(crate::io_error)?;
                        let metadata = file.metadata().map_err(crate::io_error)?;
                        if !metadata.is_file() || metadata.len() == 0 {
                            return Err(crate::error(
                                ErrorCode::InvalidCandidate,
                                "Image ist leer oder keine reguläre Datei",
                            ));
                        }
                        Ok(())
                    })();
                    check.map_err(|mut e| {
                        e.details = serde_json::json!({"path":path,"cause":e.details});
                        e
                    })?;
                }
                Ok(LinuxCandidate {
                    id: crate::volumes::candidate_id(volume_id, reference),
                    title: reference.title.clone(),
                    volume_id,
                    filesystem_uuid: reference.filesystem_uuid.clone(),
                    kernel_path: reference.kernel_path.clone(),
                    initramfs_paths: reference.initramfs_paths.clone(),
                    kernel_parameters: reference.kernel_parameters.clone(),
                    validated: true,
                    evidence: vec![
                        EvidenceRef {
                            source: Source::Linux,
                            reference: "existing_grub_config_and_images_verified".into(),
                        },
                        EvidenceRef {
                            source: Source::Volumes,
                            reference: format!("volume:{volume_id}:files_verified"),
                        },
                    ],
                })
            })();
            match checked {
                Ok(candidate) => snapshot.linux_candidates.push(candidate),
                Err(error) => {
                    errors.push(serde_json::json!({"title":reference.title,"error":error}))
                }
            }
        }
        Err(crate::error(ErrorCode::IncompleteSnapshot,"Linux-Kandidaten nur aus lesbaren bestehenden Dateien geprüft; nicht lesbare Linux-Dateisysteme bleiben unbekannt").with_details(serde_json::json!({"referenced_candidates":references.len(),"rejected":errors})))
    }
}
