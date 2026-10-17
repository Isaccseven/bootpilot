use boot_core::*;
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;
pub fn validate_path_components(reparse_flags: &[bool]) -> BootResult<()> {
    if reparse_flags.iter().any(|v| *v) {
        return Err(crate::error(
            ErrorCode::StorageUnsafe,
            "Umgeleiteter Speicherpfad wird abgewiesen",
        ));
    }
    Ok(())
}
pub struct ProtectedStore {
    root: PathBuf,
    #[cfg(windows)]
    _pins: Vec<crate::native::Handle>,
}
impl ProtectedStore {
    /// Caller identity comes from the authenticated process token; never from protocol JSON.
    pub fn open(caller_sid: &str) -> BootResult<Self> {
        #[cfg(windows)]
        {
            use crate::native::*;
            use windows::Win32::{
                Storage::FileSystem::*, System::Com::CoTaskMemFree, UI::Shell::*,
            };
            use windows::core::PCWSTR;
            let path = unsafe {
                SHGetKnownFolderPath(&FOLDERID_ProgramData, KF_FLAG_DEFAULT, None)
                    .map_err(win_error)?
            };
            let root = unsafe { path.to_string().map_err(crate::io_error) };
            unsafe {
                CoTaskMemFree(Some(path.0.cast()));
            }
            let mut root = PathBuf::from(root?);
            let mut pins = pin_path(&root)?;
            let descriptor =
                SecurityDescriptor::new("O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)")?;
            let attributes = descriptor.attributes();
            for component in [
                "BootPilot".to_string(),
                format!("caller-{:x}", Sha256::digest(caller_sid.as_bytes())),
            ] {
                root.push(component);
                if let Err(e) =
                    unsafe { CreateDirectoryW(PCWSTR(wide(&root).as_ptr()), Some(&attributes)) }
                    && e.code().0 as u32 != 0x800700b7
                {
                    return Err(win_error(e));
                }
                let opened = pin_path(&root)?;
                verify_security(opened.last().unwrap().0)?;
                pins.extend(opened);
            }
            Ok(Self { root, _pins: pins })
        }
        #[cfg(not(windows))]
        {
            let _ = caller_sid;
            Err(crate::error(
                ErrorCode::NotImplemented,
                "Geschützter Windows-Speicher erforderlich",
            ))
        }
    }
    fn path(&self, id: Uuid, kind: &str) -> PathBuf {
        self.root.join(format!("{id}.{kind}.json"))
    }
    fn read<T: DeserializeOwned>(&self, path: &Path) -> BootResult<T> {
        #[cfg(windows)]
        {
            let pins = crate::native::pin_path(path)?;
            verify_security(pins.last().unwrap().0)?;
            let data = crate::native::read_verified(path, 4 * 1024 * 1024)?;
            serde_json::from_slice(&data).map_err(crate::io_error)
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Err(crate::error(
                ErrorCode::NotImplemented,
                "Windows erforderlich",
            ))
        }
    }
    fn create<T: Serialize>(&self, path: &Path, value: &T) -> BootResult<()> {
        use std::io::Write;
        let bytes = serde_json::to_vec(value).map_err(crate::io_error)?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(crate::error(
                ErrorCode::InvalidRequest,
                "Speicherdatensatz zu groß",
            ));
        }
        // Publish a complete, flushed immutable record. A crash while writing the
        // staging file cannot leave a truncated authoritative backup or journal.
        let staging = self.root.join(format!("{}.pending", Uuid::new_v4()));
        #[cfg(windows)]
        let mut file = unsafe {
            use crate::native::*;
            use std::os::windows::io::FromRawHandle;
            use windows::{Win32::Storage::FileSystem::*, core::PCWSTR};
            let descriptor = SecurityDescriptor::new("O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)")?;
            let attributes = descriptor.attributes();
            let handle = CreateFileW(
                PCWSTR(wide(&staging).as_ptr()),
                FILE_GENERIC_WRITE.0 | READ_CONTROL.0,
                FILE_SHARE_READ,
                Some(&attributes),
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
            .map_err(|e| {
                if e.code().0 as u32 == 0x80070050 || e.code().0 as u32 == 0x800700b7 {
                    crate::error(ErrorCode::PlanConsumed, "Datensatz existiert bereits")
                } else {
                    win_error(e)
                }
            })?;
            std::fs::File::from_raw_handle(handle.0)
        };
        #[cfg(not(windows))]
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
            .map_err(crate::io_error)?;
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            verify_security(windows::Win32::Foundation::HANDLE(file.as_raw_handle()))?;
        }
        file.write_all(&bytes).map_err(crate::io_error)?;
        file.sync_all().map_err(crate::io_error)?;
        drop(file);
        #[cfg(windows)]
        {
            use crate::native::*;
            use windows::{Win32::Storage::FileSystem::*, core::PCWSTR};
            // No REPLACE_EXISTING: concurrent or consumed plans cannot be overwritten.
            unsafe {
                MoveFileExW(
                    PCWSTR(wide(&staging).as_ptr()),
                    PCWSTR(wide(path).as_ptr()),
                    MOVEFILE_WRITE_THROUGH,
                )
            }
            .map_err(|e| {
                let _ = std::fs::remove_file(&staging);
                if [0x80070050, 0x800700b7].contains(&(e.code().0 as u32)) {
                    crate::error(ErrorCode::PlanConsumed, "Datensatz existiert bereits")
                } else {
                    win_error(e)
                }
            })
        }
        #[cfg(not(windows))]
        {
            std::fs::hard_link(&staging, path).map_err(crate::io_error)?;
            std::fs::remove_file(&staging).map_err(crate::io_error)
        }
    }
}
impl PlanStore for ProtectedStore {
    fn save_plan(&mut self, plan: &ChangePlan) -> BootResult<()> {
        if plan.schema_version != SCHEMA_VERSION || plan.state != PlanState::Ready {
            return Err(crate::error(
                ErrorCode::InvalidSchema,
                "Nur neue Pläne mit aktuellem Schema speicherbar",
            ));
        }
        self.create(&self.path(plan.id, "plan"), plan)
    }
    fn load_plan(&self, id: Uuid) -> BootResult<ChangePlan> {
        let mut plan: ChangePlan = self.read(&self.path(id, "plan"))?;
        if plan.id != id || plan.schema_version != SCHEMA_VERSION {
            return Err(crate::error(
                ErrorCode::InvalidSchema,
                "Planidentität stimmt nicht überein",
            ));
        }
        if self
            .path(id, "backup")
            .try_exists()
            .map_err(crate::io_error)?
        {
            plan.state = if self
                .path(id, "outcome")
                .try_exists()
                .map_err(crate::io_error)?
            {
                PlanState::Finished
            } else {
                PlanState::Started
            };
        }
        Ok(plan)
    }
    fn begin_backup(
        &mut self,
        plan: &ChangePlan,
        originals: &[(Target, TargetValue)],
    ) -> BootResult<BackupRecord> {
        let stored = self.load_plan(plan.id)?;
        if stored.state != PlanState::Ready {
            return Err(crate::error(
                ErrorCode::PlanConsumed,
                "Plan wurde bereits begonnen",
            ));
        }
        if &stored != plan {
            return Err(crate::error(
                ErrorCode::Conflict,
                "Plan weicht vom autoritativen Speicher ab",
            ));
        }
        if plan.expires_at <= chrono::Utc::now() {
            return Err(crate::error(ErrorCode::PlanExpired, "Plan ist abgelaufen"));
        }
        if originals.len() != plan.changes.len() {
            return Err(crate::error(
                ErrorCode::BackupFailed,
                "Unvollständige Originalsicherung",
            ));
        }
        let mut targets = Vec::new();
        for change in &plan.changes {
            let matches: Vec<_> = originals
                .iter()
                .filter(|(t, _)| t == &change.target)
                .collect();
            if matches.len() != 1 || matches[0].1 != change.expected {
                return Err(crate::error(
                    ErrorCode::Conflict,
                    "Sicherungswert stimmt nicht mit Plan überein",
                ));
            }
            targets.push(BackupTarget {
                target: change.target,
                original: change.expected.clone(),
                applied: change.replacement.clone(),
                original_checksum: boot_core::plan::target_checksum(&change.expected),
                applied_checksum: boot_core::plan::target_checksum(&change.replacement),
            });
        }
        let backup = BackupRecord {
            schema_version: SCHEMA_VERSION,
            id: plan.id,
            plan_id: plan.id,
            created_at: chrono::Utc::now(),
            request: plan.request.clone(),
            targets,
            journal: Vec::new(),
            outcome: None,
        };
        // Atomic create_new permanently consumes the plan, before the first target write.
        self.create(&self.path(backup.id, "backup"), &backup)?;
        Ok(backup)
    }
    fn record_step(&mut self, id: Uuid, step: &JournalStep) -> BootResult<()> {
        let backup = self.load_backup(id)?;
        if backup.outcome.is_some() {
            return Err(crate::error(
                ErrorCode::PlanConsumed,
                "Journal bereits abgeschlossen",
            ));
        }
        if backup
            .journal
            .last()
            .is_some_and(|v| v.sequence >= step.sequence)
            || !backup.targets.iter().any(|t| t.target == step.target)
        {
            return Err(crate::error(
                ErrorCode::InvalidRequest,
                "Ungültige Journalreihenfolge oder Ziel",
            ));
        }
        self.create(&self.path(id, &format!("step-{:010}", step.sequence)), step)
    }
    fn finish_plan(&mut self, id: Uuid, outcome: &ApplyReport) -> BootResult<()> {
        let backup = self.load_backup(id)?;
        if outcome.schema_version != SCHEMA_VERSION
            || outcome.plan_id != backup.plan_id
            || outcome.backup_id != Some(id)
            || outcome.boot_tested
        {
            return Err(crate::error(
                ErrorCode::InvalidRequest,
                "Ergebnis passt nicht zur Sicherung",
            ));
        }
        self.create(&self.path(id, "outcome"), outcome)
    }
    fn list_backups(&self) -> BootResult<Vec<BackupRecord>> {
        let mut backups = Vec::new();
        for entry in std::fs::read_dir(&self.root).map_err(crate::io_error)? {
            let entry = entry.map_err(crate::io_error)?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(id) = name.strip_suffix(".backup.json") {
                backups.push(self.load_backup(Uuid::parse_str(id).map_err(crate::io_error)?)?);
            }
        }
        backups.sort_by_key(|v| v.created_at);
        Ok(backups)
    }
    fn load_backup(&self, id: Uuid) -> BootResult<BackupRecord> {
        let mut backup: BackupRecord = self.read(&self.path(id, "backup"))?;
        if backup.id != id || backup.plan_id != id || backup.schema_version != SCHEMA_VERSION {
            return Err(crate::error(
                ErrorCode::InvalidBackup,
                "Sicherungsidentität stimmt nicht überein",
            ));
        }
        let prefix = format!("{id}.step-");
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(&self.root).map_err(crate::io_error)? {
            let entry = entry.map_err(crate::io_error)?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&prefix) && name.ends_with(".json") {
                paths.push(entry.path());
            }
        }
        paths.sort();
        for path in paths {
            backup.journal.push(self.read(&path)?);
        }
        if self
            .path(id, "outcome")
            .try_exists()
            .map_err(crate::io_error)?
        {
            backup.outcome = Some(self.read(&self.path(id, "outcome"))?);
        }
        Ok(backup)
    }
}
#[cfg(windows)]
fn verify_security(handle: windows::Win32::Foundation::HANDLE) -> BootResult<()> {
    verify_object_security(
        handle,
        windows::Win32::Security::Authorization::SE_FILE_OBJECT,
    )
}
#[cfg(windows)]
pub(crate) fn verify_object_security(
    handle: windows::Win32::Foundation::HANDLE,
    object_type: windows::Win32::Security::Authorization::SE_OBJECT_TYPE,
) -> BootResult<()> {
    unsafe {
        use crate::native::*;
        use windows::Win32::Security::{Authorization::*, *};
        let mut owner = PSID::default();
        let mut dacl = std::ptr::null_mut();
        let mut sd = PSECURITY_DESCRIPTOR::default();
        GetSecurityInfo(
            handle,
            object_type,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            Some(&mut owner),
            None,
            Some(&mut dacl),
            None,
            Some(&mut sd),
        )
        .ok()
        .map_err(win_error)?;
        let _descriptor = SecurityDescriptor(sd);
        let allowed = |sid: &str| sid == "S-1-5-18" || sid == "S-1-5-32-544";
        if !allowed(&sid_string(owner)?) || dacl.is_null() {
            return Err(crate::error(
                ErrorCode::StorageUnsafe,
                "Speichereigentümer oder DACL ist nicht geschützt",
            ));
        }
        let mut admin = false;
        let mut system = false;
        for i in 0..(*dacl).AceCount {
            let mut ace = std::ptr::null_mut();
            GetAce(dacl, i as u32, &mut ace).map_err(win_error)?;
            let header = &*(ace as *const ACE_HEADER);
            if header.AceType != 0 {
                return Err(crate::error(
                    ErrorCode::StorageUnsafe,
                    "Nicht unterstützte Speicher-ACL",
                ));
            }
            let ace = &*(ace as *const ACCESS_ALLOWED_ACE);
            let sid = sid_string(PSID((&ace.SidStart as *const u32).cast_mut().cast()))?;
            if !allowed(&sid) {
                return Err(crate::error(
                    ErrorCode::StorageUnsafe,
                    "Speicher gewährt einem normalen Benutzer Zugriff",
                ));
            }
            admin |= sid == "S-1-5-32-544";
            system |= sid == "S-1-5-18";
        }
        if !admin || !system {
            return Err(crate::error(
                ErrorCode::StorageUnsafe,
                "Speicher-ACL ist unvollständig",
            ));
        }
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn precreated_user_writable_store_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let pins = crate::native::pin_path(dir.path()).unwrap();
        assert_eq!(
            verify_security(pins.last().unwrap().0).unwrap_err().code,
            ErrorCode::StorageUnsafe
        );
    }
}
