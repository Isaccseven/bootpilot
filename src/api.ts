import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  ApplyReport,
  BackupRecord,
  BootError,
  ChangePlan,
  Finding,
  IntegrationStatus,
  PlanRequest,
  RegistrationResult,
  Snapshot,
} from "./types";
export function asBootError(error: unknown): BootError {
  if (
    typeof error === "object" &&
    error !== null &&
    "message" in error &&
    "code" in error
  )
    return error as BootError;
  if (typeof error === "string") {
    try {
      const parsed: unknown = JSON.parse(error);
      if (
        typeof parsed === "object" &&
        parsed !== null &&
        "message" in parsed &&
        "code" in parsed
      )
        return parsed as BootError;
    } catch {
      /* Plain-text errors are also supported. */
    }
  }
  return {
    code: "connection_error",
    message: error instanceof Error ? error.message : String(error),
    details: null,
  };
}
async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri())
    throw {
      code: "desktop_unavailable",
      message: "Systemzugriff ist nur in der BootPilot Windows-App verfügbar.",
      details:
        "Diese Browseransicht kann keine Bootdaten erfassen. Öffne die installierte Windows-App.",
    } satisfies BootError;
  try {
    return args ? await invoke<T>(command, args) : await invoke<T>(command);
  } catch (error) {
    throw asBootError(error);
  }
}
export const api = {
  getStatus: () => call<Snapshot | null>("get_status"),
  refresh: () => call<Snapshot>("refresh"),
  diagnose: () => call<Finding[]>("diagnose"),
  prepare: (request: PlanRequest) => call<ChangePlan>("prepare", { request }),
  apply: (planId: string) => call<ApplyReport>("apply", { planId }),
  listBackups: () => call<BackupRecord[]>("list_backups"),
  getIntegrationStatus: () => call<IntegrationStatus>("get_integration_status"),
  registerCodex: () => call<RegistrationResult>("register_codex"),
};
