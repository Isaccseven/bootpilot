// JSON contract mirrored from crates/boot-core/src/model.rs. No UI write targets.
export type CollectionState =
  "complete" | "partial" | "unavailable" | "unknown";
export type Source =
  | "firmware"
  | "bcd"
  | "boot_current"
  | "secure_boot"
  | "bitlocker"
  | "grub"
  | "linux"
  | "volumes";
export interface BootError {
  code: string;
  message: string;
  details: unknown;
}
export type Target = "firmware_order" | "grub_main" | "grub_user";
export type TargetValue =
  | { kind: "firmware_order"; value: string[] }
  | { kind: "file_bytes"; value: number[] };
export type PlanRequest =
  | { kind: "order"; entry_ids: string[] }
  | { kind: "menu"; default_entry_id: string; timeout_seconds: number }
  | { kind: "cachyos"; candidate_id: string; title: string }
  | { kind: "restore"; backup_id: string };
export interface EvidenceRef {
  source: Source;
  reference: string;
}
export interface FirmwareEntry {
  id: string;
  title: string;
  loader_path: string | null;
}
export interface MenuEntry {
  id: string;
  title: string;
  kind: "windows" | "linux" | "installer" | "other";
}
export interface GrubDocument {
  main_bytes: number[];
  user_bytes: number[];
  entries: MenuEntry[];
  default_entry_id: string | null;
  timeout_seconds: number | null;
  write_supported: boolean;
  write_block_reason: string | null;
}
export interface LinuxCandidate {
  id: string;
  title: string;
  volume_id: string;
  filesystem_uuid: string;
  kernel_path: string;
  initramfs_paths: string[];
  kernel_parameters: string[];
  validated: boolean;
  evidence: EvidenceRef[];
}
export interface VolumeInfo {
  volume_id: string | null;
  partition_id: string | null;
  paths: string[];
  filesystem: string | null;
  filesystem_uuid: string | null;
  size_bytes: number | null;
  disk_number: number | null;
  partition_number: number | null;
  gpt_type: string | null;
}
export interface Snapshot {
  schema_version: number;
  id: string;
  captured_at: string;
  sources: Record<Source, CollectionState>;
  firmware_entries: FirmwareEntry[];
  firmware_order: string[];
  boot_current: {
    uefi_number: number;
    description: string | null;
    bcd_id: string | null;
  } | null;
  boot_next: number | null;
  loader_info: string | null;
  loader_entry_selected: string | null;
  volumes: VolumeInfo[];
  secure_boot: boolean | null;
  bitlocker: boolean | null;
  grub: GrubDocument | null;
  grub_location: { main_path: string; user_path: string } | null;
  linux_candidates: LinuxCandidate[];
  source_errors: { source: Source; error: BootError }[];
}
export interface Finding {
  code: string;
  severity: "info" | "warning" | "error";
  certainty: "evidence" | "hypothesis";
  statement: string;
  evidence: EvidenceRef[];
  open_check: string | null;
}
export interface ChangePlan {
  schema_version: number;
  id: string;
  created_at: string;
  expires_at: string;
  snapshot_id: string;
  request: PlanRequest;
  diffs: {
    target: Target;
    description: string;
    before: string;
    after: string;
  }[];
  preconditions: { target: Target; expected_checksum: string }[];
  changes: {
    target: Target;
    expected: TargetValue;
    replacement: TargetValue;
  }[];
  state: "ready" | "started" | "finished";
}
export interface JournalStep {
  sequence: number;
  target: Target;
  phase:
    | "intent"
    | "written"
    | "verified"
    | "rollback_intent"
    | "rolled_back"
    | "failed";
  at: string;
  detail: string | null;
}
export interface ApplyReport {
  schema_version: number;
  plan_id: string;
  backup_id: string | null;
  outcome: "applied" | "rolled_back" | "partial";
  verified_steps: JournalStep[];
  remaining_deviations: {
    target: Target;
    description: string;
    expected: TargetValue | null;
    actual: TargetValue | null;
  }[];
  boot_tested: boolean;
}
export interface BackupRecord {
  schema_version: number;
  id: string;
  plan_id: string;
  created_at: string;
  request: PlanRequest;
  targets: {
    target: Target;
    original: TargetValue;
    applied: TargetValue;
    original_checksum: string;
    applied_checksum: string;
  }[];
  journal: JournalStep[];
  outcome: ApplyReport | null;
}
export interface IntegrationStatus {
  configured: boolean;
  mcp_path: string | null;
  connection_verified: boolean;
  details: string;
}
export interface RegistrationResult {
  backup_path: string;
  config_path: string;
  restart_required: boolean;
}
