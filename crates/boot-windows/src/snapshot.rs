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
pub struct WindowsSystem {
    authorized: bool,
    location: Option<GrubLocation>,
    #[cfg(windows)]
    files: Option<(crate::write::GuardedFile, crate::write::GuardedFile)>,
}
impl WindowsSystem {
    pub(crate) fn for_transaction() -> Self {
        Self {
            authorized: true,
            ..Self::default()
        }
    }
    #[cfg(windows)]
    fn file(&mut self, target: Target) -> BootResult<&mut crate::write::GuardedFile> {
        if self.files.is_none() {
            let location = self.location.as_ref().ok_or_else(|| {
                super::error(
                    ErrorCode::IncompleteSnapshot,
                    "Autoritatives GRUB-Ziel fehlt.",
                )
            })?;
            self.files = Some((
                crate::write::GuardedFile::open(std::path::Path::new(&location.main_path))?,
                crate::write::GuardedFile::open(std::path::Path::new(&location.user_path))?,
            ));
        }
        let (main, user) = self.files.as_mut().unwrap();
        Ok(if target == Target::GrubMain {
            main
        } else {
            user
        })
    }
}
impl SystemAccess for WindowsSystem {
    fn read_snapshot(&mut self) -> BootResult<Snapshot> {
        #[cfg(windows)]
        {
            self.files = None;
        }
        let snapshot = collect_snapshot()?;
        self.location = if snapshot.sources.grub == CollectionState::Complete {
            snapshot.grub_location.clone()
        } else {
            None
        };
        Ok(snapshot)
    }
    fn read_target(&mut self, target: &Target) -> BootResult<TargetValue> {
        match target {
            Target::FirmwareOrder => {
                let snapshot = collect_snapshot()?;
                if snapshot.sources.bcd != CollectionState::Complete
                    || snapshot.sources.firmware != CollectionState::Complete
                {
                    return Err(super::error(
                        ErrorCode::IncompleteSnapshot,
                        "Firmware ist nicht vollständig erfasst.",
                    ));
                }
                Ok(TargetValue::FirmwareOrder(snapshot.firmware_order))
            }
            #[cfg(windows)]
            Target::GrubMain | Target::GrubUser => {
                self.file(*target)?.read().map(TargetValue::FileBytes)
            }
            #[cfg(not(windows))]
            _ => Err(super::error(
                ErrorCode::NotImplemented,
                "Windows erforderlich",
            )),
        }
    }
    fn write_target(
        &mut self,
        target: &Target,
        expected: &TargetValue,
        replacement: &TargetValue,
    ) -> TargetWriteResult {
        if !self.authorized {
            return Err(WriteFailure::NotWritten(super::error(
                ErrorCode::InvalidRequest,
                "Schreiben erfordert eine geprüfte Helfertransaktion.",
            )));
        }
        #[cfg(windows)]
        {
            crate::write::reject_grub2win().map_err(WriteFailure::NotWritten)?;
            match (target, expected, replacement) {
                (
                    Target::FirmwareOrder,
                    TargetValue::FirmwareOrder(old),
                    TargetValue::FirmwareOrder(new),
                ) => crate::bcd::write_order(old, new),
                (
                    Target::GrubMain | Target::GrubUser,
                    TargetValue::FileBytes(old),
                    TargetValue::FileBytes(new),
                ) => self
                    .file(*target)
                    .map_err(WriteFailure::NotWritten)?
                    .replace(old, new),
                _ => Err(WriteFailure::NotWritten(super::error(
                    ErrorCode::InvalidRequest,
                    "Ziel und Werttyp stimmen nicht überein.",
                ))),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (target, expected, replacement);
            Err(WriteFailure::NotWritten(super::error(
                ErrorCode::NotImplemented,
                "Windows erforderlich",
            )))
        }
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
                    Err(e) => {
                        let mut error = crate::io_error(e);
                        error.details["path"] = serde_json::json!(main);
                        failures.push(error);
                    }
                }
            }
        }
        grub_from_discovery(snapshot, found, failures, read_verified)
    }
    fn grub_from_discovery(
        snapshot: &mut Snapshot,
        mut found: Vec<PathBuf>,
        failures: Vec<BootError>,
        read: impl Fn(&std::path::Path, usize) -> BootResult<Vec<u8>>,
    ) -> BootResult<()> {
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
        let main_bytes = read(&main, 512 * 1024)?;
        let user_bytes = read(&user, 512 * 1024)?;
        snapshot.grub_location = Some(GrubLocation {
            main_path: main.to_string_lossy().into(),
            user_path: user.to_string_lossy().into(),
        });
        snapshot.grub = Some(parse_grub(&main_bytes, &user_bytes)?);
        if failures.is_empty() {
            Ok(())
        } else {
            snapshot.grub.as_mut().unwrap().write_supported = false;
            snapshot.grub.as_mut().unwrap().write_block_reason =
                Some("GRUB-Installation ist wegen unlesbarer Suchpfade nicht eindeutig".into());
            Err(crate::error(
                ErrorCode::IncompleteSnapshot,
                "Lesbare GRUB-Konfiguration gefunden, Eindeutigkeit der Suche nicht bestätigt",
            )
            .with_details(serde_json::json!({"errors":failures})))
        }
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
    #[cfg(test)]
    mod review_tests {
        use super::*;
        #[test]
        fn readable_grub_plus_denied_volume_is_partial_and_retains_document() {
            let mut snapshot = Snapshot::new(Uuid::new_v4(), Utc::now());
            let denied = crate::error(ErrorCode::AccessDenied, "Zweites Volume nicht lesbar");
            let result = grub_from_discovery(
                &mut snapshot,
                vec![PathBuf::from(r"C:\grub2\grub.cfg")],
                vec![denied],
                |_, _| Ok(b"existing configuration".to_vec()),
            );
            assert_eq!(result.unwrap_err().code, ErrorCode::IncompleteSnapshot);
            let grub = snapshot.grub.unwrap();
            assert_eq!(grub.main_bytes, b"existing configuration");
            assert!(!grub.write_supported);
            assert!(grub.write_block_reason.unwrap().contains("eindeutig"));
        }
        #[test]
        fn single_readable_grub_without_discovery_errors_is_complete() {
            let mut snapshot = Snapshot::new(Uuid::new_v4(), Utc::now());
            assert!(
                grub_from_discovery(
                    &mut snapshot,
                    vec![PathBuf::from(r"C:\grub2\grub.cfg")],
                    Vec::new(),
                    |_, _| Ok(Vec::new())
                )
                .is_ok()
            );
            assert!(snapshot.grub.is_some());
        }
    }
}
