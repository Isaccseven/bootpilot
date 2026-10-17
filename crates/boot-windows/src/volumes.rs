use boot_core::*;
use uuid::Uuid;
/// Accept only a volume-GUID root discovered by Windows, never a caller path.
pub fn volume_id(path: &str) -> Option<Uuid> {
    let body = path.strip_prefix(r"\\?\Volume{")?.strip_suffix("}\\")?;
    Uuid::parse_str(body).ok()
}
#[cfg(windows)]
pub(crate) fn collect(snapshot: &mut Snapshot) -> BootResult<()> {
    use serde::Deserialize;
    use wmi::WMIConnection;
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Partition {
        disk_number: u32,
        partition_number: u32,
        guid: Option<String>,
        gpt_type: Option<String>,
        size: u64,
        access_paths: Option<Vec<String>>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Volume {
        #[serde(rename = "DeviceID")]
        device_id: String,
        drive_letter: Option<String>,
        file_system: Option<String>,
        serial_number: Option<u32>,
        capacity: Option<u64>,
    }
    let connection = WMIConnection::with_namespace_path("ROOT\\Microsoft\\Windows\\Storage")
        .map_err(crate::native::wmi_error)?;
    let partitions: Vec<Partition> = connection
        .raw_query(
            "SELECT DiskNumber,PartitionNumber,Guid,GptType,Size,AccessPaths FROM MSFT_Partition",
        )
        .map_err(crate::native::wmi_error)?;
    let connection = WMIConnection::new().map_err(crate::native::wmi_error)?;
    let volumes: Vec<Volume> = connection
        .raw_query("SELECT DeviceID,DriveLetter,FileSystem,SerialNumber,Capacity FROM Win32_Volume")
        .map_err(crate::native::wmi_error)?;
    for p in partitions {
        let mut paths = p.access_paths.unwrap_or_default();
        let volume = volumes
            .iter()
            .find(|v| paths.iter().any(|p| p.eq_ignore_ascii_case(&v.device_id)));
        if let Some(v) = volume
            && let Some(drive) = &v.drive_letter
        {
            let path = format!("{drive}\\");
            if !paths.contains(&path) {
                paths.push(path)
            }
        }
        snapshot.volumes.push(VolumeInfo {
            volume_id: paths.iter().find_map(|p| volume_id(p)),
            partition_id: p.guid.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
            filesystem: volume.and_then(|v| v.file_system.clone()),
            filesystem_uuid: volume
                .and_then(|v| v.serial_number)
                .map(|n| format!("{:04X}-{:04X}", n >> 16, n & 0xffff)),
            size_bytes: Some(p.size),
            disk_number: Some(p.disk_number),
            partition_number: Some(p.partition_number),
            gpt_type: p.gpt_type.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
            paths,
        });
    }
    for v in volumes {
        if snapshot
            .volumes
            .iter()
            .any(|p| p.paths.iter().any(|p| p.eq_ignore_ascii_case(&v.device_id)))
        {
            continue;
        }
        let mut paths = vec![v.device_id.clone()];
        if let Some(drive) = v.drive_letter {
            paths.push(format!("{drive}\\"));
        }
        snapshot.volumes.push(VolumeInfo {
            volume_id: volume_id(&v.device_id),
            partition_id: None,
            paths,
            filesystem: v.file_system,
            filesystem_uuid: v
                .serial_number
                .map(|n| format!("{:04X}-{:04X}", n >> 16, n & 0xffff)),
            size_bytes: v.capacity,
            disk_number: None,
            partition_number: None,
            gpt_type: None,
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxReference {
    pub title: String,
    pub filesystem_uuid: String,
    pub kernel_path: String,
    pub initramfs_paths: Vec<String>,
    pub kernel_parameters: Vec<String>,
}
/// Extract only literal existing GRUB references; never execute or expand its language.
pub fn linux_references(bytes: &[u8]) -> Vec<LinuxReference> {
    let text = String::from_utf8_lossy(bytes);
    let mut references = Vec::new();
    let mut fs_uuid = None;
    let mut kernel = None;
    let mut initrds = Vec::new();
    let mut params = Vec::new();
    let mut title = String::from("Linux aus bestehender GRUB-Konfiguration");
    for line in text.lines() {
        let words: Vec<_> = line.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }
        if words[0] == "menuentry" {
            fs_uuid = None;
            kernel = None;
            initrds.clear();
            params.clear();
            title = line
                .split('\'')
                .nth(1)
                .filter(|s| !s.is_empty())
                .unwrap_or("Linux aus bestehender GRUB-Konfiguration")
                .to_string();
        }
        if (words[0] == "search" || (words[0] == "if" && words.get(1) == Some(&"search")))
            && words.contains(&"--fs-uuid")
        {
            fs_uuid = words
                .iter()
                .rev()
                .map(|v| v.trim_matches(['\'', '"', ';']))
                .find(|v| {
                    v.len() == 9
                        && v.as_bytes()[4] == b'-'
                        && v.chars()
                            .enumerate()
                            .all(|(i, c)| i == 4 || c.is_ascii_hexdigit())
                })
                .map(str::to_string);
        }
        if words[0] == "linux" && words.len() > 1 {
            kernel = Some(words[1].trim_matches(['\'', '"']).to_string());
            params = words[2..].iter().map(|s| s.to_string()).collect();
        }
        if words[0] == "initrd" {
            initrds = words[1..]
                .iter()
                .map(|v| v.trim_matches(['\'', '"']).to_string())
                .collect();
        }
        if line.trim() == "}" {
            if let (Some(filesystem_uuid), Some(kernel_path)) = (fs_uuid.take(), kernel.take()) {
                references.push(LinuxReference {
                    title: title.clone(),
                    filesystem_uuid,
                    kernel_path,
                    initramfs_paths: initrds.clone(),
                    kernel_parameters: params.clone(),
                });
            }
            initrds.clear();
            params.clear();
        }
    }
    references
}
/// UUIDv8 from SHA-256 of canonical discovered identity and exact boot inputs.
/// A refreshed snapshot must resolve the same candidate selected in an earlier preview.
pub fn candidate_id(volume_id: Uuid, reference: &LinuxReference) -> Uuid {
    use sha2::{Digest, Sha256};
    let canonical = serde_json::json!([
        "bootpilot-linux-candidate-v1",
        volume_id,
        reference.filesystem_uuid.to_ascii_uppercase(),
        reference.kernel_path,
        reference.initramfs_paths,
        reference.kernel_parameters
    ]);
    let digest = Sha256::digest(serde_json::to_vec(&canonical).expect("JSON value serialization"));
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
