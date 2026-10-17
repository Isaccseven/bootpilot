use boot_core::{BootError, ErrorCode};
use boot_service::BootService;
use serde::Serialize;
use std::process::ExitCode;

// Exit codes: 0 success (including empty status), 1 runtime/I/O failure,
// 2 usage, 3 no capture, 4 UAC cancelled, 5 access denied.
fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let json = args.iter().any(|value| value == "--json");
    if args
        .first()
        .is_some_and(|command| command == "register-codex")
    {
        let path = match args.as_slice() {
            [_, flag, path] if flag == "--mcp-path" => Some(path),
            [_, flag, path, json_flag] if flag == "--mcp-path" && json_flag == "--json" => {
                Some(path)
            }
            _ => None,
        };
        let result = path.ok_or_else(|| BootError::new(ErrorCode::InvalidRequest,
            "Aufruf: boot-cli register-codex --mcp-path <absoluter-installierter-Pfad> [--json]"))
            .and_then(|path| boot_service::integration::register_codex_at(
                &boot_service::integration::codex_config_path()?, std::path::Path::new(path)))
            .and_then(|result| print_json(&result));
        return finish(result, json);
    }
    if matches!(args.as_slice(), [command] if command == "integration-status")
        || matches!(args.as_slice(), [command, flag] if command == "integration-status" && flag == "--json")
    {
        return finish(
            boot_service::integration::get_integration_status()
                .and_then(|status| print_json(&status)),
            json,
        );
    }
    let command = match args.as_slice() {
        [command] if ["status", "refresh", "diagnose", "backups"].contains(&command.as_str()) => {
            command.as_str()
        }
        [command, flag]
            if flag == "--json"
                && ["status", "refresh", "diagnose", "backups"].contains(&command.as_str()) =>
        {
            command.as_str()
        }
        _ => {
            emit_error(
                &BootError::new(
                    ErrorCode::InvalidRequest,
                    "Aufruf: boot-cli status|refresh|diagnose|backups|integration-status [--json]; register-codex --mcp-path <Pfad> [--json]",
                ),
                json,
            );
            return ExitCode::from(2);
        }
    };
    let service = BootService::new();
    let result = match command {
        "status" => service.get_status().and_then(|snapshot| {
            if json {
                print_json(&snapshot)
            } else if let Some(snapshot) = snapshot {
                println!(
                    "Erfassung vom {} (UTC); Vollständigkeit je Quelle beachten.",
                    snapshot.captured_at.to_rfc3339()
                );
                print_json(&snapshot)
            } else {
                println!("Noch keine Erfassung. Mit boot-cli refresh ausdrücklich aktualisieren.");
                Ok(())
            }
        }),
        "refresh" => service.refresh().and_then(|snapshot| {
            if !json {
                println!(
                    "Erfassung vom {} (UTC); Vollständigkeit und Quellenfehler beachten.",
                    snapshot.captured_at.to_rfc3339()
                );
            }
            print_json(&snapshot)
        }),
        "diagnose" => service
            .diagnose()
            .and_then(|findings| print_json(&findings)),
        "backups" => service
            .list_backups()
            .and_then(|backups| print_json(&backups)),
        _ => unreachable!("validated command"),
    };
    finish(result, json)
}
fn finish(result: Result<(), BootError>, json: bool) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            emit_error(&error, json);
            ExitCode::from(match error.code {
                ErrorCode::InvalidRequest => 2,
                ErrorCode::IncompleteSnapshot => 3,
                ErrorCode::UacCancelled => 4,
                ErrorCode::AccessDenied => 5,
                _ => 1,
            })
        }
    }
}
fn print_json(value: &impl Serialize) -> Result<(), BootError> {
    let json = serde_json::to_string_pretty(value).map_err(|_| {
        BootError::new(
            ErrorCode::Io,
            "Die Antwort konnte nicht serialisiert werden.",
        )
    })?;
    println!("{json}");
    Ok(())
}
fn emit_error(error: &BootError, json: bool) {
    if json {
        eprintln!(
            "{}",
            serde_json::to_string(error).expect("BootError is serializable")
        );
    } else {
        eprintln!("BootPilot: {}", error.message);
    }
}
