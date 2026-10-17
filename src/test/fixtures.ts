import type { Snapshot, ChangePlan, ApplyReport, BackupRecord } from "../types";
export const snapshot: Snapshot = {
  schema_version: 1,
  id: "snapshot-1",
  captured_at: "2026-10-17T12:00:00Z",
  sources: {
    firmware: "complete",
    bcd: "complete",
    boot_current: "complete",
    secure_boot: "unknown",
    bitlocker: "unknown",
    grub: "complete",
    linux: "partial",
    volumes: "complete",
  },
  firmware_entries: [
    {
      id: "win",
      title: "Windows Boot Manager",
      loader_path: "\\EFI\\Microsoft\\Boot\\bootmgfw.efi",
    },
    {
      id: "grub",
      title: "Grub2Win",
      loader_path: "\\EFI\\grub2win\\g2bootmgr\\gnugrub.kernel64.efi",
    },
  ],
  firmware_order: ["win", "grub"],
  boot_current: null,
  boot_next: null,
  loader_info: null,
  loader_entry_selected: null,
  volumes: [],
  secure_boot: null,
  bitlocker: null,
  grub: {
    main_bytes: [],
    user_bytes: [],
    entries: [
      { id: "windows", title: "Windows", kind: "windows" },
      { id: "installer", title: "CachyOS Live ISO", kind: "installer" },
    ],
    default_entry_id: "windows",
    timeout_seconds: 5,
    write_supported: true,
    write_block_reason: null,
  },
  grub_location: {
    main_path: "C:\\grub2\\grub.cfg",
    user_path: "C:\\grub2\\user.cfg",
  },
  linux_candidates: [],
  source_errors: [],
};
export function plan(expires = Date.now() + 15 * 60000): ChangePlan {
  return {
    schema_version: 1,
    id: "plan-1",
    created_at: new Date().toISOString(),
    expires_at: new Date(expires).toISOString(),
    // Prepare collects a separate authoritative scan, even if targets match.
    snapshot_id: "authoritative-snapshot-2",
    request: { kind: "menu", default_entry_id: "windows", timeout_seconds: 5 },
    diffs: [
      {
        target: "grub_main",
        description: "Wartezeit",
        before: "set timeout=3",
        after: "set timeout=5",
      },
    ],
    preconditions: [
      { target: "grub_main", expected_checksum: "sha256-verified" },
    ],
    changes: [
      {
        target: "grub_main",
        expected: { kind: "file_bytes", value: [1] },
        replacement: { kind: "file_bytes", value: [2] },
      },
    ],
    state: "ready",
  };
}
export const partial: ApplyReport = {
  schema_version: 1,
  plan_id: "plan-1",
  backup_id: "backup-1",
  outcome: "partial",
  verified_steps: [],
  remaining_deviations: [
    {
      target: "grub_main",
      description: "Wartezeit konnte nicht zurückgenommen werden.",
      expected: null,
      actual: null,
    },
  ],
  boot_tested: false,
};
export const backup: BackupRecord = {
  schema_version: 1,
  id: "backup-1",
  plan_id: "plan-1",
  created_at: snapshot.captured_at,
  request: plan().request,
  targets: [],
  journal: [],
  outcome: null,
};
