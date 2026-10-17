use boot_core::*;
use std::{mem::size_of, path::Path};
use windows::{
    Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
        Storage::FileSystem::*,
        System::{Threading::*, WindowsProgramming::*},
    },
    core::{PCWSTR, PWSTR},
};
pub(crate) fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.as_ref().encode_wide().chain([0]).collect()
}
pub(crate) struct Handle(pub HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
pub(crate) fn win_error(e: windows::core::Error) -> BootError {
    let code = if e.code().0 as u32 == 0x80070005 || e.code().0 as u32 == 0x80070522 {
        ErrorCode::AccessDenied
    } else {
        ErrorCode::Io
    };
    crate::error(code, "Windows-API-Zugriff fehlgeschlagen").with_details(
        serde_json::json!({"hresult":format!("{:08x}",e.code().0),"reason":e.to_string()}),
    )
}
pub(crate) fn wmi_error(e: wmi::WMIError) -> BootError {
    let text = e.to_string();
    let code = if text.contains("80070005")
        || text.contains("80041003")
        || text.to_lowercase().contains("access")
        || text.to_lowercase().contains("verweigert")
    {
        ErrorCode::AccessDenied
    } else {
        ErrorCode::Io
    };
    crate::error(code, "WMI-Quelle konnte nicht gelesen werden")
        .with_details(serde_json::json!({"reason":text}))
}
pub(crate) struct Privilege {
    token: Handle,
    previous: TOKEN_PRIVILEGES,
}
impl Drop for Privilege {
    fn drop(&mut self) {
        unsafe {
            let _ = AdjustTokenPrivileges(self.token.0, false, Some(&self.previous), 0, None, None);
        }
    }
}
pub(crate) fn enable_privilege(name: &str) -> BootResult<Privilege> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY | TOKEN_ADJUST_PRIVILEGES,
            &mut token,
        )
        .map_err(win_error)?;
        let token = Handle(token);
        let mut luid = LUID::default();
        LookupPrivilegeValueW(None, PCWSTR(wide(name).as_ptr()), &mut luid).map_err(win_error)?;
        let requested = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        let mut previous = TOKEN_PRIVILEGES::default();
        let mut length = 0;
        SetLastError(ERROR_SUCCESS);
        AdjustTokenPrivileges(
            token.0,
            false,
            Some(&requested),
            size_of::<TOKEN_PRIVILEGES>() as u32,
            Some(&mut previous),
            Some(&mut length),
        )
        .map_err(win_error)?;
        if GetLastError() == ERROR_NOT_ALL_ASSIGNED {
            return Err(crate::error(
                ErrorCode::AccessDenied,
                "Erforderliches Leserecht ist im Token nicht verfügbar",
            ));
        }
        Ok(Privilege { token, previous })
    }
}
pub(crate) fn variable(name: &str, namespace: &str) -> BootResult<Vec<u8>> {
    unsafe {
        let _privilege = enable_privilege("SeSystemEnvironmentPrivilege")?;
        let mut data = vec![0u8; 65536];
        let len = GetFirmwareEnvironmentVariableExW(
            PCWSTR(wide(name).as_ptr()),
            PCWSTR(wide(namespace).as_ptr()),
            Some(data.as_mut_ptr().cast()),
            data.len() as u32,
            None,
        );
        if len == 0 {
            let e = GetLastError();
            return Err(if e == ERROR_ENVVAR_NOT_FOUND {
                crate::error(ErrorCode::NotFound, "UEFI-Variable ist nicht gesetzt")
            } else {
                win_error(windows::core::Error::from_thread())
            });
        }
        data.truncate(len as usize);
        Ok(data)
    }
}
/// Pin every ancestor without delete sharing; inspect the opened object, including final reparse state.
pub(crate) fn pin_path(path: &Path) -> BootResult<Vec<Handle>> {
    if !path.is_absolute() {
        return Err(crate::error(
            ErrorCode::StorageUnsafe,
            "Absoluter Pfad erforderlich",
        ));
    }
    let mut paths: Vec<_> = path.ancestors().filter(|p| p.has_root()).collect();
    paths.reverse();
    let mut handles = Vec::new();
    for path in paths {
        unsafe {
            let handle = Handle(
                CreateFileW(
                    PCWSTR(wide(path).as_ptr()),
                    FILE_READ_ATTRIBUTES.0 | READ_CONTROL.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                    None,
                )
                .map_err(win_error)?,
            );
            let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
            let result = GetFileInformationByHandleEx(
                handle.0,
                FileAttributeTagInfo,
                (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
                size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
            );
            match result {
                Ok(()) => {}
                Err(e) if [0x80070057, 0x80070001].contains(&(e.code().0 as u32)) => {
                    // FAT can reject FileAttributeTagInfo. Query attributes on the same
                    // pinned handle; never fall back to following the path by name.
                    let mut basic = BY_HANDLE_FILE_INFORMATION::default();
                    GetFileInformationByHandle(handle.0, &mut basic).map_err(win_error)?;
                    info.FileAttributes = basic.dwFileAttributes;
                }
                Err(e) => return Err(win_error(e)),
            }
            crate::store::validate_path_components(&[info.FileAttributes
                & FILE_ATTRIBUTE_REPARSE_POINT.0
                != 0])?;
            handles.push(handle);
        }
    }
    Ok(handles)
}
pub(crate) fn read_verified(path: &Path, max: usize) -> BootResult<Vec<u8>> {
    use std::io::Read;
    let _pins = pin_path(path)?;
    let mut file = std::fs::File::open(path).map_err(crate::io_error)?;
    if file.metadata().map_err(crate::io_error)?.len() > max as u64 {
        return Err(crate::error(
            ErrorCode::InvalidMessage,
            "Datei überschreitet Größenlimit",
        ));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(crate::io_error)?;
    if bytes.len() > max {
        return Err(crate::error(
            ErrorCode::InvalidMessage,
            "Datei überschreitet Größenlimit",
        ));
    }
    Ok(bytes)
}
pub(crate) struct SecurityDescriptor(pub PSECURITY_DESCRIPTOR);
impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(Some(HLOCAL(self.0.0)));
        }
    }
}
impl SecurityDescriptor {
    pub fn new(sddl: &str) -> BootResult<Self> {
        unsafe {
            let mut p = PSECURITY_DESCRIPTOR::default();
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(wide(sddl).as_ptr()),
                1,
                &mut p,
                None,
            )
            .map_err(win_error)?;
            Ok(Self(p))
        }
    }
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0.0,
            bInheritHandle: false.into(),
        }
    }
}
pub(crate) fn sid_string(sid: PSID) -> BootResult<String> {
    unsafe {
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(sid, &mut text).map_err(win_error)?;
        let result = text.to_string().map_err(crate::io_error);
        LocalFree(Some(HLOCAL(text.0.cast())));
        result
    }
}
pub(crate) fn process_sid(process: HANDLE) -> BootResult<String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).map_err(win_error)?;
        let token = Handle(token);
        let mut length = 0;
        let _ = GetTokenInformation(token.0, TokenUser, None, 0, &mut length);
        let mut bytes = vec![0u64; (length as usize).div_ceil(8)];
        GetTokenInformation(
            token.0,
            TokenUser,
            Some(bytes.as_mut_ptr().cast()),
            length,
            &mut length,
        )
        .map_err(win_error)?;
        sid_string((*(bytes.as_ptr() as *const TOKEN_USER)).User.Sid)
    }
}

#[cfg(test)]
mod native_tests {
    use super::*;
    #[test]
    fn pinned_normal_file_remains_readable() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("source");
        std::fs::write(&file, b"original").unwrap();
        assert_eq!(read_verified(&file, 32).unwrap(), b"original");
        assert!(read_verified(&file, 2).is_err());
    }
    #[test]
    fn ancestor_junction_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let link = dir.path().join("link");
        let output = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "junction fixture creation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::write(target.join("data"), b"data").unwrap();
        assert_eq!(
            pin_path(&link).err().unwrap().code,
            ErrorCode::StorageUnsafe
        );
        assert_eq!(
            pin_path(&link.join("data")).err().unwrap().code,
            ErrorCode::StorageUnsafe
        );
        // Remove only the junction itself; keep tempfile's normal target cleanup separate.
        std::fs::remove_dir(&link).unwrap();
    }
}
