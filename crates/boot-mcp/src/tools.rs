use boot_core::*;
use boot_service::BootService;
use chrono::{DateTime, Utc};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::schema_for_type, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Freshness is advisory: no cached status is an authorization for planning/writing.
pub const STALE_AFTER_SECONDS: i64 = 300;

#[derive(Serialize, JsonSchema)]
pub struct StatusOutput {
    pub schema_version: u32,
    pub snapshot: Option<Snapshot>,
    pub captured_at: Option<DateTime<Utc>>,
    pub age_seconds: Option<i64>,
    pub is_stale: bool,
}
impl StatusOutput {
    fn new(snapshot: Option<Snapshot>) -> Self {
        let captured_at = snapshot.as_ref().map(|s| s.captured_at);
        let age = captured_at.map(|at| Utc::now().signed_duration_since(at).num_seconds());
        Self {
            schema_version: SCHEMA_VERSION,
            snapshot,
            captured_at,
            age_seconds: age.map(|value| value.max(0)),
            is_stale: age.is_none_or(|value| !(0..=STALE_AFTER_SECONDS).contains(&value)),
        }
    }
}
#[derive(Serialize, JsonSchema)]
pub struct Output<T> {
    pub schema_version: u32,
    pub data: T,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OrderInput {
    pub entry_ids: Vec<Uuid>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MenuInput {
    pub default_entry_id: String,
    pub timeout_seconds: u16,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CachyosInput {
    pub candidate_id: Uuid,
    pub title: String,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyInput {
    pub plan_id: Uuid,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RestoreInput {
    pub backup_id: Uuid,
}

#[derive(Clone)]
pub struct BootMcp {
    service: Arc<BootService>,
    tool_router: ToolRouter<Self>,
}
impl BootMcp {
    pub fn new(service: BootService) -> Self {
        Self {
            service: Arc::new(service),
            tool_router: Self::tool_router(),
        }
    }
    async fn run<T: Serialize + Send + 'static>(
        &self,
        summary: &'static str,
        operation: impl FnOnce(&BootService) -> BootResult<T> + Send + 'static,
    ) -> CallToolResult {
        let service = self.service.clone();
        match tokio::task::spawn_blocking(move || operation(&service)).await {
            Ok(Ok(data)) => success(
                Output {
                    schema_version: SCHEMA_VERSION,
                    data,
                },
                summary,
            ),
            Ok(Err(error)) => failure(error),
            Err(_) => failure(BootError::new(
                ErrorCode::Io,
                "Der Hintergrundaufruf wurde unterbrochen.",
            )),
        }
    }
}

#[tool_router]
impl BootMcp {
    #[tool(description = "Letzte lokal gespeicherte Erfassung mit Erfassungszeit, Aktualität und Vollständigkeit je Quelle lesen. Ohne Cache: null. Keine UAC-Abfrage.", output_schema = schema_for_type::<StatusOutput>(), annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false))]
    async fn boot_get_status(&self) -> CallToolResult {
        match self.service.get_status() {
            Ok(snapshot) => {
                let summary = match &snapshot {
                    Some(value) => format!(
                        "Erfassung vom {}. Vollständigkeit je Quelle und Aktualität beachten.",
                        value.captured_at.to_rfc3339()
                    ),
                    None => {
                        "Noch keine Erfassung. Mit boot_refresh ausdrücklich aktualisieren.".into()
                    }
                };
                success(StatusOutput::new(snapshot), &summary)
            }
            Err(error) => failure(error),
        }
    }

    #[tool(description = "Neue Systemerfassung über den geschützten Helfer starten und lokalen Cache aktualisieren. Kann eine interaktive Windows-UAC-Abfrage erfordern. Keine Bootkonfiguration ändern.", output_schema = schema_for_type::<Output<Snapshot>>(), annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false))]
    async fn boot_refresh(&self) -> CallToolResult {
        self.run(
            "Neue Erfassung gespeichert. Vollständigkeit und Quellenfehler beachten.",
            BootService::refresh,
        )
        .await
    }

    #[tool(description = "Belegte Befunde, mögliche Ursachen und offene Prüfungen aus der letzten lokalen Erfassung lesen. Keine UAC-Abfrage; ohne Erfassung ein Fehler.", output_schema = schema_for_type::<Output<Vec<Finding>>>(), annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false))]
    async fn boot_diagnose(&self) -> CallToolResult {
        self.run(
            "Diagnose der letzten Erfassung. Hinweise enthalten Belege und offene Prüfungen.",
            BootService::diagnose,
        )
        .await
    }

    #[tool(description = "Reihenfolge vorhandener Firmware-Einträge als lokalen Plan vorbereiten. Der Helfer erfasst und validiert frisch; UAC möglich. Ändert noch keine Bootkonfiguration.", output_schema = schema_for_type::<Output<ChangePlan>>(), annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false))]
    async fn boot_plan_order(&self, Parameters(input): Parameters<OrderInput>) -> CallToolResult {
        self.run(
            "Plan vorbereitet. Vorschau prüfen; erst boot_apply_plan kann Änderungen anwenden.",
            move |service| {
                service.prepare(PlanRequest::Order {
                    entry_ids: input.entry_ids,
                })
            },
        )
        .await
    }

    #[tool(description = "Standardauswahl und Wartezeit als lokalen Menüplan vorbereiten. Frische Validierung durch den Helfer, UAC möglich. Ändert noch keine Bootkonfiguration.", output_schema = schema_for_type::<Output<ChangePlan>>(), annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false))]
    async fn boot_plan_menu(&self, Parameters(input): Parameters<MenuInput>) -> CallToolResult {
        self.run(
            "Menüplan vorbereitet. Vorschau vor dem Anwenden prüfen.",
            move |service| {
                service.prepare(PlanRequest::Menu {
                    default_entry_id: input.default_entry_id,
                    timeout_seconds: input.timeout_seconds,
                })
            },
        )
        .await
    }

    #[tool(description = "Einen bereits geprüften CachyOS-Kandidaten als lokalen Menüplan vorbereiten. Keine freien Dateipfade oder Kernelbefehle. Frische Helferprüfung, UAC möglich; noch keine Bootänderung.", output_schema = schema_for_type::<Output<ChangePlan>>(), annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false))]
    async fn boot_plan_cachyos(
        &self,
        Parameters(input): Parameters<CachyosInput>,
    ) -> CallToolResult {
        self.run(
            "CachyOS-Plan vorbereitet. Vorschau vor dem Anwenden prüfen.",
            move |service| {
                service.prepare(PlanRequest::Cachyos {
                    candidate_id: input.candidate_id,
                    title: input.title,
                })
            },
        )
        .await
    }

    #[tool(description = "Einen zuvor vorbereiteten, gültigen Plan nach Prüfung seiner Vorschau anwenden. Schreibend und potenziell destruktiv: verändert Bootkonfiguration. Erfordert UAC und erneute Zustandsprüfung. Toolannotation ist keine Genehmigung. Kein Neustart.", output_schema = schema_for_type::<Output<ApplyReport>>(), annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false))]
    async fn boot_apply_plan(&self, Parameters(input): Parameters<ApplyInput>) -> CallToolResult {
        self.run("Ausführung beendet. Ergebnis, verifizierte Schritte und verbleibende Abweichungen im Bericht prüfen; kein Boot-Test erfolgt.", move |service| service.apply(input.plan_id)).await
    }

    #[tool(description = "Eigene Sicherungen einschließlich unvollständiger Journale lesen. Liest den geschützten Speicher über den Helfer; UAC möglich. Keine Bootkonfiguration ändern.", output_schema = schema_for_type::<Output<Vec<BackupRecord>>>(), annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false))]
    async fn boot_list_backups(&self) -> CallToolResult {
        self.run(
            "Sicherungsliste gelesen. Unvollständige Journale bleiben sichtbar.",
            BootService::list_backups,
        )
        .await
    }

    #[tool(description = "Gezielte Rücknahme einer eigenen Sicherung als lokalen Plan vorbereiten. Prüft Besitz und aktuellen Zustand frisch, UAC möglich. Noch keine Rücknahme; kein pauschaler BCD-Import.", output_schema = schema_for_type::<Output<ChangePlan>>(), annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false))]
    async fn boot_plan_restore(
        &self,
        Parameters(input): Parameters<RestoreInput>,
    ) -> CallToolResult {
        self.run(
            "Rücknahmeplan vorbereitet. Vorschau vor dem Anwenden prüfen.",
            move |service| {
                service.prepare(PlanRequest::Restore {
                    backup_id: input.backup_id,
                })
            },
        )
        .await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for BootMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("bootpilot", env!("CARGO_PKG_VERSION")))
            .with_instructions("BootPilot: Status ist eine lokale Erfassung mit Zeitstempel. Refresh und geschützter Speicher können UAC erfordern. Planer erzeugen lokale Vorschauen, Apply schreibt erst nach eigener Validierung. Toolannotationen gewähren keine Freigabe. Keine Neustarts.")
    }
}

fn success(value: impl Serialize, summary: &str) -> CallToolResult {
    match serde_json::to_value(value) {
        Ok(value) => {
            let mut result = CallToolResult::structured(value);
            result.content = vec![ContentBlock::text(summary)];
            result
        }
        Err(_) => failure(BootError::new(
            ErrorCode::Io,
            "Die Antwort konnte nicht serialisiert werden.",
        )),
    }
}
fn failure(error: BootError) -> CallToolResult {
    let summary = error.message.clone();
    let mut result = CallToolResult::structured_error(
        serde_json::json!({ "schema_version": SCHEMA_VERSION, "error": error }),
    );
    result.content = vec![ContentBlock::text(summary)];
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initialize_lists_expected_tools() {
        let server = BootMcp::new(BootService::with_client_and_cache(
            boot_windows::BrokerClient::new(),
            None,
        ));
        let tools = server.tool_router.list_all();
        assert_eq!(tools.len(), 9);
        for tool in tools {
            assert!(tool.name.starts_with("boot_"));
            assert!(tool.output_schema.is_some());
        }
    }
    #[test]
    fn missing_plan_id_is_invalid() {
        assert!(serde_json::from_value::<ApplyInput>(serde_json::json!({})).is_err());
        assert!(
            serde_json::from_value::<ApplyInput>(
                serde_json::json!({"plan_id": Uuid::new_v4(), "shell": "unexpected"})
            )
            .is_err()
        );
    }
    #[test]
    fn status_has_freshness_and_completeness() {
        let none = serde_json::to_value(StatusOutput::new(None)).unwrap();
        assert!(none["snapshot"].is_null());
        assert!(none["captured_at"].is_null());
        assert!(none["age_seconds"].is_null());
        assert_eq!(none["is_stale"], true);
        let value = serde_json::to_value(StatusOutput::new(Some(Snapshot::new(
            Uuid::new_v4(),
            Utc::now(),
        ))))
        .unwrap();
        assert_eq!(value["snapshot"]["sources"]["firmware"], "unknown");
        assert_eq!(value["is_stale"], false);
    }
    #[test]
    fn errors_are_tool_errors_not_success_text() {
        let result = failure(BootError::new(ErrorCode::UacCancelled, "UAC abgebrochen"));
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content.unwrap()["error"]["code"],
            "uac_cancelled"
        );
    }
}
