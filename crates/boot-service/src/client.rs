//! User-local snapshot persistence, entirely separate from the protected plan store.
use boot_core::*;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MAX_CACHE_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) fn default_cache_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|root| {
        PathBuf::from(root)
            .join("BootPilot")
            .join("snapshot-v1.json")
    })
}
pub(crate) fn validate_snapshot(snapshot: &Snapshot) -> BootResult<()> {
    if snapshot.schema_version != SCHEMA_VERSION {
        return Err(BootError::new(
            ErrorCode::InvalidSchema,
            "Unbekannte Version der Erfassung.",
        ));
    }
    Ok(())
}
pub(crate) fn read_cache(path: &Path) -> BootResult<Option<Snapshot>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(cache_error(error)),
    };
    let mut bytes = Vec::new();
    file.take(MAX_CACHE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(cache_error)?;
    if bytes.len() as u64 > MAX_CACHE_BYTES {
        return Err(BootError::new(
            ErrorCode::InvalidSchema,
            "Die lokale Erfassung ist zu groß.",
        ));
    }
    let snapshot: Snapshot = serde_json::from_slice(&bytes).map_err(|error| {
        BootError::new(
            ErrorCode::InvalidSchema,
            "Die gespeicherte Erfassung ist ungültig.",
        )
        .with_details(serde_json::json!({"reason": error.to_string()}))
    })?;
    validate_snapshot(&snapshot)?;
    Ok(Some(snapshot))
}
pub(crate) fn write_cache(path: &Path, snapshot: &Snapshot) -> BootResult<()> {
    let directory = path
        .parent()
        .ok_or_else(|| BootError::new(ErrorCode::Io, "Cacheverzeichnis fehlt."))?;
    fs::create_dir_all(directory).map_err(cache_error)?;
    let temporary = directory.join(format!("snapshot-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let bytes = serde_json::to_vec(snapshot).map_err(cache_error)?;
        if bytes.len() as u64 > MAX_CACHE_BYTES {
            return Err(BootError::new(
                ErrorCode::Io,
                "Die Erfassung ist zu groß für den lokalen Cache.",
            ));
        }
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(cache_error)?;
        file.write_all(&bytes).map_err(cache_error)?;
        file.sync_all().map_err(cache_error)?;
        drop(file);
        fs::rename(&temporary, path).map_err(cache_error)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
fn cache_error(error: impl std::fmt::Display) -> BootError {
    BootError::new(
        ErrorCode::Io,
        "Die lokale Erfassung konnte nicht gelesen oder gespeichert werden.",
    )
    .with_details(serde_json::json!({"reason": error.to_string()}))
}
