use boot_core::*;
use boot_service::{
    BootService,
    integration::{self, IntegrationStatus, RegistrationResult},
};
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

type Service<'a> = State<'a, Arc<BootService>>;

async fn run<T: Send + 'static>(
    operation: impl FnOnce() -> BootResult<T> + Send + 'static,
) -> BootResult<T> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| BootError::new(ErrorCode::Io, "Der Hintergrundaufruf wurde unterbrochen."))?
}
#[tauri::command]
pub async fn get_status(service: Service<'_>) -> BootResult<Option<Snapshot>> {
    let service = service.inner().clone();
    run(move || service.get_status()).await
}
#[tauri::command]
pub async fn refresh(service: Service<'_>) -> BootResult<Snapshot> {
    let service = service.inner().clone();
    run(move || service.refresh()).await
}
#[tauri::command]
pub async fn diagnose(service: Service<'_>) -> BootResult<Vec<Finding>> {
    let service = service.inner().clone();
    run(move || service.diagnose()).await
}
#[tauri::command]
pub async fn prepare(service: Service<'_>, request: PlanRequest) -> BootResult<ChangePlan> {
    let service = service.inner().clone();
    run(move || service.prepare(request)).await
}
#[tauri::command]
pub async fn apply(service: Service<'_>, plan_id: Uuid) -> BootResult<ApplyReport> {
    let service = service.inner().clone();
    run(move || service.apply(plan_id)).await
}
#[tauri::command]
pub async fn list_backups(service: Service<'_>) -> BootResult<Vec<BackupRecord>> {
    let service = service.inner().clone();
    run(move || service.list_backups()).await
}
#[tauri::command]
pub async fn get_integration_status() -> BootResult<IntegrationStatus> {
    run(integration::get_integration_status).await
}
#[tauri::command]
pub async fn register_codex() -> BootResult<RegistrationResult> {
    run(integration::register_codex).await
}
