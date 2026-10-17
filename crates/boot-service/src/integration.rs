//! Narrow Codex registration shared by the ordinary-user GUI and installed CLI.
use boot_core::{BootError, BootResult, ErrorCode};
use fs2::FileExt;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
use toml_edit::{DocumentMut, Table, value};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct RegistrationResult {
    pub backup_path: String,
    pub config_path: String,
    pub restart_required: bool,
}

#[derive(Debug, Serialize)]
pub struct IntegrationStatus {
    pub configured: bool,
    pub mcp_path: Option<String>,
    pub connection_verified: bool,
    pub details: String,
}

pub fn codex_config_path() -> BootResult<PathBuf> {
    let home = std::env::var_os("CODEX_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join(".codex")))
        .ok_or_else(|| invalid("Das Codex-Konfigurationsverzeichnis ist unbekannt."))?;
    if !home.is_absolute() {
        return Err(invalid("CODEX_HOME muss ein absoluter Pfad sein."));
    }
    Ok(home.join("config.toml"))
}

/// The webview cannot supply a path. Sidecars are siblings of the installed app.
pub fn sibling_mcp_path() -> BootResult<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|parent| parent.join("boot-mcp.exe")))
        .ok_or_else(|| invalid("Der installierte MCP-Pfad konnte nicht bestimmt werden."))
}

pub fn register_codex() -> BootResult<RegistrationResult> {
    register_codex_at(&codex_config_path()?, &sibling_mcp_path()?)
}

/// Only the bootpilot table is edited. The original file is locked against writes
/// and replacement throughout backup and update (including unrelated editors on
/// Windows); a second registration fails instead of overwriting a racing writer.
pub fn register_codex_at(config: &Path, mcp: &Path) -> BootResult<RegistrationResult> {
    validate_mcp(mcp)?;
    if !config.is_absolute() || config == mcp {
        return Err(invalid(
            "Ein eigener absoluter Konfigurationspfad ist erforderlich.",
        ));
    }
    let parent = config
        .parent()
        .ok_or_else(|| invalid("Ungültiger Konfigurationspfad."))?;
    fs::create_dir_all(parent).map_err(|_| io_error())?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ: deny other write opens and file replacement while held.
        options.share_mode(1);
    }
    let mut file = options.open(config).map_err(|_| conflict())?;
    file.try_lock_exclusive().map_err(|_| conflict())?;
    let mut original = String::new();
    file.read_to_string(&mut original).map_err(|_| io_error())?;
    let has_bom = original.starts_with('\u{feff}');
    let parse_input = original.strip_prefix('\u{feff}').unwrap_or(&original);
    let crlf = parse_input.contains("\r\n");
    if crlf && parse_input.replace("\r\n", "").contains('\n') {
        return Err(invalid(
            "Gemischte Zeilenenden in der Codex-Konfiguration; zur Erhaltung nichts ersetzt.",
        ));
    }
    let mut doc = parse_input
        .parse::<DocumentMut>()
        .map_err(|_| invalid("Die Codex-Konfiguration ist kein gültiges TOML; nichts ersetzt."))?;
    if doc.get("mcp_servers").is_some_and(|item| !item.is_table()) {
        return Err(invalid("mcp_servers muss eine TOML-Tabelle sein."));
    }
    if doc.get("mcp_servers").is_none() {
        doc["mcp_servers"] = Table::new().into();
    }
    if doc["mcp_servers"]
        .get("bootpilot")
        .is_some_and(|item| !item.is_table())
    {
        return Err(invalid(
            "mcp_servers.bootpilot muss eine TOML-Tabelle sein.",
        ));
    }
    if doc["mcp_servers"].get("bootpilot").is_none() {
        doc["mcp_servers"]["bootpilot"] = Table::new().into();
    }
    let table = doc["mcp_servers"]["bootpilot"]
        .as_table_mut()
        .expect("validated table");
    // These transport alternatives conflict with the local stdio command.
    for key in [
        "url",
        "http_headers",
        "env_http_headers",
        "bearer_token_env_var",
    ] {
        table.remove(key);
    }
    let command = value(
        mcp.to_str()
            .ok_or_else(|| invalid("Der MCP-Pfad ist nicht Unicode."))?,
    );
    set_preserving_comment(table, "command", command);
    set_preserving_comment(table, "args", value(toml_edit::Array::new()));
    // A formerly disabled registration must become usable after this explicit action.
    if table.get("enabled").is_some() {
        set_preserving_comment(table, "enabled", value(true));
    }
    // toml_edit emits LF; retain Windows CRLF and an existing UTF-8 BOM.
    let mut edited = doc.to_string();
    if crlf {
        edited = edited.replace("\r\n", "\n").replace('\n', "\r\n");
    }
    if has_bom {
        edited.insert(0, '\u{feff}');
    }
    let backup = parent.join(format!("config.toml.bootpilot-{}.bak", Uuid::new_v4()));
    let mut backup_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup)
        .map_err(|_| io_error())?;
    backup_file
        .write_all(original.as_bytes())
        .and_then(|()| backup_file.sync_all())
        .map_err(|_| io_error())?;
    if edited != original {
        let replace = |file: &mut File, bytes: &[u8]| -> std::io::Result<()> {
            file.seek(SeekFrom::Start(0))?;
            file.write_all(bytes)?;
            file.set_len(bytes.len() as u64)?;
            file.sync_all()
        };
        if replace(&mut file, edited.as_bytes()).is_err() {
            if replace(&mut file, original.as_bytes()).is_err() {
                return Err(BootError::new(ErrorCode::RollbackFailed,
                    "Registrierung fehlgeschlagen; die vorherige Konfiguration liegt in der Sicherung.")
                    .with_details(json!({"backup_path": backup})));
            }
            return Err(io_error());
        }
    }
    Ok(RegistrationResult {
        backup_path: backup.to_string_lossy().into_owned(),
        config_path: config.to_string_lossy().into_owned(),
        restart_required: true,
    })
}

fn set_preserving_comment(table: &mut Table, key: &str, mut replacement: toml_edit::Item) {
    if let (Some(old), Some(new)) = (
        table.get(key).and_then(|v| v.as_value()),
        replacement.as_value_mut(),
    ) {
        *new.decor_mut() = old.decor().clone();
    }
    table[key] = replacement;
}

fn validate_mcp(path: &Path) -> BootResult<()> {
    if !path.is_absolute() || path.file_name().is_none_or(|name| name != "boot-mcp.exe") {
        return Err(invalid(
            "Ein absoluter installierter boot-mcp.exe-Pfad ist erforderlich.",
        ));
    }
    let mut file = File::open(path)
        .map_err(|_| invalid("boot-mcp.exe ist nicht vorhanden oder nicht lesbar."))?;
    let mut magic = [0; 2];
    if !file.metadata().map_err(|_| io_error())?.is_file()
        || file.read_exact(&mut magic).is_err()
        || magic != *b"MZ"
    {
        return Err(invalid(
            "Der MCP-Pfad muss auf eine Windows-Programmdatei zeigen.",
        ));
    }
    let mut offset = [0; 4];
    let mut pe_header = [0; 24];
    let pe_valid = file.seek(SeekFrom::Start(0x3c)).is_ok()
        && file.read_exact(&mut offset).is_ok()
        && file
            .seek(SeekFrom::Start(u64::from(u32::from_le_bytes(offset))))
            .is_ok()
        && file.read_exact(&mut pe_header).is_ok()
        && pe_header[..4] == *b"PE\0\0"
        && u16::from_le_bytes([pe_header[4], pe_header[5]]) == 0x8664
        && u16::from_le_bytes([pe_header[22], pe_header[23]]) & 0x2002 == 0x0002;
    if !pe_valid {
        return Err(invalid(
            "boot-mcp.exe ist keine ausführbare Windows-x64-PE-Datei.",
        ));
    }
    Ok(())
}

pub fn get_integration_status() -> BootResult<IntegrationStatus> {
    integration_status_at(&codex_config_path()?, &sibling_mcp_path()?)
}

pub fn integration_status_at(config: &Path, sibling_mcp: &Path) -> BootResult<IntegrationStatus> {
    let original = match fs::read_to_string(config) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return Err(io_error()),
    };
    let doc = original
        .parse::<DocumentMut>()
        .map_err(|_| invalid("Die Codex-Konfiguration ist kein gültiges TOML."))?;
    let table = doc.get("mcp_servers").and_then(|i| i.get("bootpilot"));
    let mcp_path = table
        .and_then(|i| i.get("command"))
        .and_then(|i| i.as_str())
        .map(str::to_owned);
    let configured = mcp_path.as_ref().is_some_and(|p| {
        fs::canonicalize(p)
            .ok()
            .zip(fs::canonicalize(sibling_mcp).ok())
            .is_some_and(|(a, b)| a == b)
    }) && table
        .and_then(|i| i.get("args"))
        .and_then(|i| i.as_array())
        .is_some_and(|a| a.is_empty())
        && table.and_then(|i| i.get("url")).is_none()
        && table
            .and_then(|i| i.get("enabled"))
            .and_then(|i| i.as_bool())
            != Some(false);
    // Never execute an arbitrary command found in the user's TOML. Probe only
    // the known installed sibling, independently of registration.
    let (connection_verified, details) = match probe_mcp(sibling_mcp, Duration::from_secs(5)) {
        Ok(()) => (true, "Direkter MCP-Test: initialize, tools/list und boot_get_status erfolgreich. Die Verbindung dieser Codex-Sitzung wurde dadurch nicht bestätigt.".into()),
        Err(error) => (false, format!("Direkter MCP-Test nicht bestätigt: {}", error.message)),
    };
    Ok(IntegrationStatus {
        configured,
        mcp_path,
        connection_verified,
        details,
    })
}

/// Bounded, read-only MCP measurement. No refresh, broker or elevation request.
pub fn probe_mcp(path: &Path, timeout: Duration) -> BootResult<()> {
    validate_mcp(path)?;
    let mut command = Command::new(path);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let child = command.spawn().map_err(|_| io_error())?;
    let mut child = ProbeChild(child);
    let stdout = child.0.stdout.take().ok_or_else(io_error)?;
    let mut stdin = child.0.stdin.take().ok_or_else(io_error)?;
    let (sender, receiver) = mpsc::sync_channel(4);
    let reader = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut bytes = Vec::new();
            // Never allocate unbounded stdout from an unresponsive sidecar.
            let result = (&mut reader).take(1_048_577).read_until(b'\n', &mut bytes);
            if result.is_err() || bytes.is_empty() || bytes.len() > 1_048_576 {
                break;
            }
            if sender
                .send(serde_json::from_slice::<Value>(&bytes).ok())
                .is_err()
            {
                break;
            }
        }
    });
    let deadline = Instant::now() + timeout;
    let mut exchange = |id: u64, method: &str, params: Value| -> BootResult<Value> {
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
        )
        .and_then(|()| stdin.flush())
        .map_err(|_| io_error())?;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let response = receiver
                .recv_timeout(remaining)
                .map_err(|_| {
                    BootError::new(ErrorCode::Timeout, "MCP antwortet nicht rechtzeitig.")
                })?
                .ok_or_else(|| invalid("MCP lieferte ungültiges JSON."))?;
            if response["id"] != id {
                continue;
            }
            if response["jsonrpc"] != "2.0" || response.get("error").is_some() {
                return Err(invalid("MCP lehnte die lesende Protokollprüfung ab."));
            }
            if id == 1 {
                writeln!(
                    stdin,
                    "{}",
                    json!({"jsonrpc":"2.0","method":"notifications/initialized"})
                )
                .and_then(|()| stdin.flush())
                .map_err(|_| io_error())?;
            }
            return Ok(response["result"].clone());
        }
    };
    let result = (|| {
        let init = exchange(
            1,
            "initialize",
            json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"bootpilot-integration-check","version":"0.1.0"}}),
        )?;
        if init["serverInfo"]["name"] != "bootpilot" {
            return Err(invalid("Unerwarteter MCP-Server."));
        }
        let tools = exchange(2, "tools/list", json!({}))?;
        if !tools["tools"]
            .as_array()
            .is_some_and(|tools| tools.iter().any(|t| t["name"] == "boot_get_status"))
        {
            return Err(invalid("Das lesende Status-Tool fehlt."));
        }
        let status = exchange(
            3,
            "tools/call",
            json!({"name":"boot_get_status","arguments":{}}),
        )?;
        if status["isError"] == true
            || status["structuredContent"]["schema_version"] != 1
            || status["structuredContent"].get("snapshot").is_none()
        {
            return Err(invalid("Die lesende Status-Abfrage wurde nicht bestätigt."));
        }
        Ok(())
    })();
    drop(exchange);
    drop(stdin);
    drop(receiver);
    let _ = child.0.kill();
    let _ = child.0.wait();
    let _ = reader.join();
    result
}
struct ProbeChild(Child);
impl Drop for ProbeChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn invalid(message: &str) -> BootError {
    BootError::new(ErrorCode::InvalidRequest, message)
}
fn conflict() -> BootError {
    BootError::new(
        ErrorCode::Conflict,
        "Die Codex-Konfiguration wird gerade bearbeitet oder ist nicht schreibbar.",
    )
}
fn io_error() -> BootError {
    BootError::new(
        ErrorCode::Io,
        "Die lokale Codex-Integration konnte nicht gelesen oder gespeichert werden.",
    )
}
