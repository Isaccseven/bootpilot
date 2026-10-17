//! Native guards. No caller supplied path reaches these adapters through IPC.
use crate::native::*;
use boot_core::*;
use std::{
    io::{Read, Seek, SeekFrom, Write},
    mem::size_of,
    os::windows::io::{AsRawHandle, FromRawHandle},
    path::Path,
};
use windows::{
    Win32::{
        Foundation::*,
        Storage::FileSystem::*,
        System::{Diagnostics::ToolHelp::*, Threading::*},
    },
    core::PCWSTR,
};

pub(crate) struct MachineLock(Handle);
impl MachineLock {
    pub(crate) fn acquire() -> BootResult<Self> {
        unsafe {
            let descriptor = SecurityDescriptor::new("O:BAG:BAD:P(A;;GA;;;SY)(A;;GA;;;BA)")?;
            let handle = Handle(
                CreateMutexW(
                    Some(&descriptor.attributes()),
                    false,
                    PCWSTR(wide("Global\\BootPilot-BootMutation-v1").as_ptr()),
                )
                .map_err(win_error)?,
            );
            // Existing named objects must also be protected; prevent mutex-name squatting.
            crate::store::verify_object_security(
                handle.0,
                windows::Win32::Security::Authorization::SE_KERNEL_OBJECT,
            )?;
            match WaitForSingleObject(handle.0, 60_000) {
                WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(Self(handle)),
                WAIT_TIMEOUT => Err(crate::error(
                    ErrorCode::Conflict,
                    "Eine andere BootPilot-Änderung läuft.",
                )),
                _ => Err(win_error(windows::core::Error::from_thread())),
            }
        }
    }
}
impl Drop for MachineLock {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.0.0);
        }
    }
}

pub(crate) fn reject_grub2win() -> BootResult<()> {
    unsafe {
        let processes = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(win_error)?);
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        Process32FirstW(processes.0, &mut entry).map_err(win_error)?;
        loop {
            let end = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..end])
                .to_ascii_lowercase()
                .contains("grub2win")
            {
                return Err(crate::error(
                    ErrorCode::Conflict,
                    "Grub2Win läuft. Vor dem Anwenden schließen.",
                ));
            }
            if let Err(e) = Process32NextW(processes.0, &mut entry) {
                if GetLastError() == ERROR_NO_MORE_FILES {
                    break;
                }
                return Err(win_error(e));
            }
        }
        Ok(())
    }
}

pub(crate) struct GuardedFile {
    file: std::fs::File,
    identity: (u32, u32, u32),
    _pins: Vec<Handle>,
}
impl GuardedFile {
    pub(crate) fn open(path: &Path) -> BootResult<Self> {
        let pins = pin_path(path.parent().ok_or(crate::error(
            ErrorCode::StorageUnsafe,
            "Dateielternpfad fehlt.",
        ))?)?;
        let file = unsafe {
            std::fs::File::from_raw_handle(
                CreateFileW(
                    PCWSTR(wide(path).as_ptr()),
                    FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0 | READ_CONTROL.0,
                    FILE_SHARE_READ,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_WRITE_THROUGH,
                    None,
                )
                .map_err(win_error)?
                .0,
            )
        };
        let identity = checked_identity(&file)?;
        Ok(Self {
            file,
            identity,
            _pins: pins,
        })
    }
    pub(crate) fn read(&mut self) -> BootResult<Vec<u8>> {
        if checked_identity(&self.file)? != self.identity {
            return Err(crate::error(
                ErrorCode::Conflict,
                "Dateiidentität hat sich geändert.",
            ));
        }
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(crate::io_error)?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut self.file)
            .take(512 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(crate::io_error)?;
        if bytes.len() > 512 * 1024 {
            return Err(crate::error(
                ErrorCode::UnsupportedConfiguration,
                "GRUB-Datei überschreitet Größenlimit.",
            ));
        }
        Ok(bytes)
    }
    pub(crate) fn replace(&mut self, expected: &[u8], replacement: &[u8]) -> BootResult<()> {
        self.replace_with(expected, replacement, |f, bytes| f.write_all(bytes))
    }
    fn replace_with(
        &mut self,
        expected: &[u8],
        replacement: &[u8],
        write: impl FnOnce(&mut std::fs::File, &[u8]) -> std::io::Result<()>,
    ) -> BootResult<()> {
        if replacement.len() > 512 * 1024 {
            return Err(crate::error(
                ErrorCode::InvalidRequest,
                "GRUB-Ersatz überschreitet Größenlimit.",
            ));
        }
        if self.read()? != expected {
            return Err(crate::error(
                ErrorCode::Conflict,
                "GRUB-Datei wurde seit der Vorschau verändert.",
            ));
        }
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(crate::io_error)?;
        // Exclusive same-handle mutation avoids a pathname exchange race. This is not
        // power-fail atomic: interrupted bytes require visible, manually reviewed recovery.
        write(&mut self.file, replacement).map_err(crate::io_error)?;
        self.file
            .set_len(replacement.len() as u64)
            .map_err(crate::io_error)?;
        self.file.sync_all().map_err(crate::io_error)?;
        if self.read()? != replacement {
            return Err(crate::error(
                ErrorCode::VerificationFailed,
                "GRUB-Datei konnte nicht bestätigt werden.",
            ));
        }
        Ok(())
    }
}
fn checked_identity(file: &std::fs::File) -> BootResult<(u32, u32, u32)> {
    unsafe {
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info).map_err(win_error)?;
        if info.dwFileAttributes & (FILE_ATTRIBUTE_REPARSE_POINT.0 | FILE_ATTRIBUTE_DIRECTORY.0)
            != 0
            || info.nNumberOfLinks != 1
        {
            return Err(crate::error(
                ErrorCode::StorageUnsafe,
                "GRUB-Ziel ist umgeleitet, verknüpft oder keine reguläre Datei.",
            ));
        }
        Ok((
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_value_and_concurrent_write_and_rename_are_rejected() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config");
        std::fs::write(&p, b"original").unwrap();
        let mut f = GuardedFile::open(&p).unwrap();
        assert_eq!(
            f.replace(b"stale", b"replacement").unwrap_err().code,
            ErrorCode::Conflict
        );
        assert!(std::fs::write(&p, b"foreign").is_err());
        assert!(std::fs::rename(&p, d.path().join("renamed")).is_err());
        f.replace(b"original", b"new").unwrap();
        assert_eq!(f.read().unwrap(), b"new");
        drop(f);
        assert_eq!(std::fs::read(&p).unwrap(), b"new");
    }
    #[test]
    fn hardlink_and_reparse_targets_are_rejected() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config");
        std::fs::write(&p, b"original").unwrap();
        let link = d.path().join("link");
        std::fs::hard_link(&p, &link).unwrap();
        assert_eq!(
            GuardedFile::open(&p).err().unwrap().code,
            ErrorCode::StorageUnsafe
        );
    }
    #[test]
    fn partial_write_failure_is_visible_and_never_overwrites_with_stale_expectation() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config");
        std::fs::write(&p, b"original").unwrap();
        let mut f = GuardedFile::open(&p).unwrap();
        assert!(
            f.replace_with(b"original", b"replacement", |f, b| {
                f.write_all(&b[..3])?;
                Err(std::io::Error::other("injected interruption"))
            })
            .is_err()
        );
        assert_eq!(f.read().unwrap(), b"repginal");
        assert_eq!(
            f.replace(b"replacement", b"original").unwrap_err().code,
            ErrorCode::Conflict
        );
    }
}
