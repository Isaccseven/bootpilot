import { act, render, screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { snapshot, plan, partial, backup } from "./test/fixtures";
const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: ipc.invoke,
  isTauri: () => true,
}));
beforeEach(() => {
  ipc.invoke.mockReset().mockImplementation((command: string) => {
    if (command === "get_status" || command === "refresh")
      return Promise.resolve(snapshot);
    if (command === "prepare") return Promise.resolve(plan());
    if (command === "apply") return Promise.resolve(partial);
    if (command === "list_backups") return Promise.resolve([backup]);
    if (command === "diagnose") return Promise.resolve([]);
    if (command === "get_integration_status")
      return Promise.resolve({
        configured: true,
        mcp_path: "C:\\BootPilot\\boot-mcp.exe",
        connection_verified: false,
        details: "Neustart erforderlich.",
      });
    return Promise.reject(new Error(command));
  });
});
async function menu() {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("Windows Boot Manager");
  await user.click(screen.getByRole("button", { name: "Bootmenü" }));
  return user;
}
describe("ehrliche Systemansicht", () => {
  it("unknown_is_not_disabled_security", async () => {
    render(<App />);
    const secure = await screen.findByRole("group", { name: "Secure Boot" });
    expect(within(secure).getByText("Unbekannt")).toBeVisible();
    expect(within(secure).queryByText("Deaktiviert")).not.toBeInTheDocument();
  });
  it("uac_cancel_keeps_last_snapshot_marked_stale", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Windows Boot Manager");
    ipc.invoke.mockRejectedValueOnce({
      code: "uac_cancelled",
      message: "Administratorabfrage abgebrochen.",
      details: { retry: "Erneut aktualisieren" },
    });
    await user.click(screen.getByRole("button", { name: "Aktualisieren" }));
    expect(
      await screen.findByText("Administratorabfrage abgebrochen."),
    ).toBeVisible();
    expect(screen.getByText("Windows Boot Manager")).toBeVisible();
    expect(screen.getByText(/Veraltet/)).toBeVisible();
  });
  it("installer_is_not_installed_linux", async () => {
    render(<App />);
    await screen.findByText("CachyOS Live ISO");
    expect(screen.getByText("Installer / Live-System")).toBeVisible();
    expect(
      screen.getByText(/Keine validierte Linux-Installation/),
    ).toBeVisible();
  });
  it("does_not_elevate_on_mount", async () => {
    render(<App />);
    await screen.findByText("Windows Boot Manager");
    expect(ipc.invoke.mock.calls.map((c) => c[0])).toEqual(["get_status"]);
  });
});
describe("sichtbarer Änderungsplan", () => {
  it("fresh_authoritative_scan_plan_can_apply_after_visible_acknowledgment", async () => {
    const user = await menu();
    await user.click(screen.getByRole("button", { name: "Aktualisieren" }));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Menüvorschau erstellen" }),
      ).toBeEnabled(),
    );
    const freshPlan = plan();
    expect(freshPlan.snapshot_id).not.toBe(snapshot.id);
    ipc.invoke.mockResolvedValueOnce(freshPlan);
    await user.click(
      screen.getByRole("button", { name: "Menüvorschau erstellen" }),
    );
    const preview = await screen.findByRole("region", {
      name: "Änderungsvorschau",
    });
    expect(within(preview).getByText(freshPlan.snapshot_id)).toBeVisible();
    expect(within(preview).getByText("set timeout=3")).toBeVisible();
    expect(within(preview).getByText("set timeout=5")).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Änderungen anwenden" }),
    ).toBeDisabled();
    expect(ipc.invoke.mock.calls.some((c) => c[0] === "apply")).toBe(false);
    const acknowledgment = screen.getByRole("checkbox", {
      name: /Vorschau geprüft/,
    });
    expect(acknowledgment).toBeEnabled();
    await user.click(acknowledgment);
    await user.click(
      screen.getByRole("button", { name: "Änderungen anwenden" }),
    );
    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("apply", {
        planId: freshPlan.id,
      }),
    );
    expect(await screen.findByText("Teilweise ausgeführt")).toBeVisible();
  });
  it("delayed_prepare_cannot_restore_plan_after_navigation", async () => {
    const user = await menu();
    let complete!: (value: ReturnType<typeof plan>) => void;
    const pending = new Promise<ReturnType<typeof plan>>((resolve) => {
      complete = resolve;
    });
    ipc.invoke.mockReturnValueOnce(pending);
    await user.click(
      screen.getByRole("button", { name: "Menüvorschau erstellen" }),
    );
    // Editing and refresh cannot race a running prepare; navigation stays usable.
    expect(
      screen.getByRole("spinbutton", { name: "Wartezeit in Sekunden" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Aktualisieren" }),
    ).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Übersicht" }));
    await user.click(screen.getByRole("button", { name: "Bootmenü" }));
    await act(async () => {
      complete(plan());
      await pending;
    });
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Menüvorschau erstellen" }),
      ).toBeEnabled(),
    );
    expect(
      screen.queryByRole("region", { name: "Änderungsvorschau" }),
    ).not.toBeInTheDocument();
    expect(ipc.invoke.mock.calls.some((c) => c[0] === "apply")).toBe(false);
  });
  it("refresh_discards_a_previously_visible_plan", async () => {
    const user = await menu();
    await user.click(
      screen.getByRole("button", { name: "Menüvorschau erstellen" }),
    );
    await screen.findByRole("region", { name: "Änderungsvorschau" });
    await user.click(screen.getByRole("button", { name: "Aktualisieren" }));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Aktualisieren" }),
      ).toBeEnabled(),
    );
    expect(
      screen.queryByRole("region", { name: "Änderungsvorschau" }),
    ).not.toBeInTheDocument();
  });
  it("validated_candidate_can_be_prepared_from_partial_linux_inventory", async () => {
    ipc.invoke.mockResolvedValueOnce({
      ...snapshot,
      linux_candidates: [
        {
          id: "candidate-1",
          title: "CachyOS",
          volume_id: "volume-1",
          filesystem_uuid: "fs-1",
          kernel_path: "/boot/vmlinuz-linux-cachyos",
          initramfs_paths: ["/boot/initramfs-linux-cachyos.img"],
          kernel_parameters: [],
          validated: true,
          evidence: [],
        },
      ],
    });
    const user = await menu();
    await user.click(
      screen.getByRole("button", { name: "CachyOS-Vorschau erstellen" }),
    );
    await screen.findByRole("region", { name: "Änderungsvorschau" });
    expect(ipc.invoke).toHaveBeenCalledWith("prepare", {
      request: {
        kind: "cachyos",
        candidate_id: "candidate-1",
        title: "CachyOS",
      },
    });
  });
  it("complete_firmware_can_be_reordered_when_separate_bcd_source_is_partial", async () => {
    ipc.invoke.mockResolvedValueOnce({
      ...snapshot,
      sources: { ...snapshot.sources, bcd: "partial" },
    });
    const user = await menu();
    await user.click(
      screen.getByRole("button", { name: "Grub2Win nach oben" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Reihenfolge als Vorschau" }),
    );
    await screen.findByRole("region", { name: "Änderungsvorschau" });
    expect(ipc.invoke).toHaveBeenCalledWith("prepare", {
      request: { kind: "order", entry_ids: ["grub", "win"] },
    });
  });
  it("failed_apply_marks_prior_snapshot_stale", async () => {
    const user = await menu();
    await user.click(
      screen.getByRole("button", { name: "Menüvorschau erstellen" }),
    );
    await screen.findByRole("region", { name: "Änderungsvorschau" });
    await user.click(
      screen.getByRole("checkbox", { name: /Vorschau geprüft/ }),
    );
    ipc.invoke.mockRejectedValueOnce({
      code: "conflict",
      message: "Konfiguration geändert. Neue Erfassung erforderlich.",
      details: null,
    });
    await user.click(
      screen.getByRole("button", { name: "Änderungen anwenden" }),
    );
    await screen.findByText(
      "Konfiguration geändert. Neue Erfassung erforderlich.",
    );
    expect(screen.getByText(/Veraltet/)).toBeVisible();
  });
  it("changing_form_discards_the_previous_plan", async () => {
    const user = await menu();
    await user.click(
      screen.getByRole("button", { name: "Menüvorschau erstellen" }),
    );
    await screen.findByRole("region", { name: "Änderungsvorschau" });
    await user.clear(
      screen.getByRole("spinbutton", { name: "Wartezeit in Sekunden" }),
    );
    expect(
      screen.queryByRole("region", { name: "Änderungsvorschau" }),
    ).not.toBeInTheDocument();
  });
  it("expires_while_preview_is_open", async () => {
    const user = await menu();
    ipc.invoke.mockResolvedValueOnce(plan(Date.now() + 500));
    await user.click(
      screen.getByRole("button", { name: "Menüvorschau erstellen" }),
    );
    await screen.findByRole("region", { name: "Änderungsvorschau" });
    await user.click(
      screen.getByRole("checkbox", { name: /Vorschau geprüft/ }),
    );
    expect(
      screen.getByRole("button", { name: "Änderungen anwenden" }),
    ).toBeEnabled();
    await waitFor(
      () =>
        expect(
          screen.getByRole("button", { name: "Änderungen anwenden" }),
        ).toBeDisabled(),
      { timeout: 2000 },
    );
  });
  it("apply_requires_visible_plan", async () => {
    const user = await menu();
    expect(
      screen.queryByRole("button", { name: "Änderungen anwenden" }),
    ).not.toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Menüvorschau erstellen" }),
    );
    const preview = await screen.findByRole("region", {
      name: "Änderungsvorschau",
    });
    expect(within(preview).getByText("set timeout=3")).toBeVisible();
    expect(within(preview).getByText("set timeout=5")).toBeVisible();
    await user.click(
      screen.getByRole("checkbox", { name: /Vorschau geprüft/ }),
    );
    await user.click(
      screen.getByRole("button", { name: "Änderungen anwenden" }),
    );
    await waitFor(() =>
      expect(ipc.invoke).toHaveBeenCalledWith("apply", { planId: "plan-1" }),
    );
  });
  it("expired_plan_cannot_apply", async () => {
    const user = await menu();
    ipc.invoke.mockResolvedValueOnce(plan(Date.now() - 1000));
    await user.click(
      screen.getByRole("button", { name: "Menüvorschau erstellen" }),
    );
    expect(await screen.findByText(/Plan abgelaufen/)).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Änderungen anwenden" }),
    ).toBeDisabled();
  });
  it("partial_result_shows_backup_and_remaining_changes", async () => {
    const user = await menu();
    await user.click(
      screen.getByRole("button", { name: "Menüvorschau erstellen" }),
    );
    await screen.findByRole("region", { name: "Änderungsvorschau" });
    await user.click(
      screen.getByRole("checkbox", { name: /Vorschau geprüft/ }),
    );
    await user.click(
      screen.getByRole("button", { name: "Änderungen anwenden" }),
    );
    expect(await screen.findByText("Teilweise ausgeführt")).toBeVisible();
    expect(screen.getByText("backup-1")).toBeVisible();
    expect(
      screen.getByText("Wartezeit konnte nicht zurückgenommen werden."),
    ).toBeVisible();
    expect(screen.getByText("Boot nicht getestet")).toBeVisible();
  });
});
describe("Sicherungen und Codex", () => {
  it("failed_backup_list_does_not_claim_loading_forever", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Windows Boot Manager");
    ipc.invoke.mockRejectedValueOnce({
      code: "io",
      message: "Sicherungen nicht lesbar.",
      details: null,
    });
    await user.click(screen.getByRole("button", { name: "Sicherungen" }));
    await screen.findByText("Sicherungen nicht lesbar.");
    expect(
      screen.queryByText("Sicherungen werden geladen …"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Sicherungen neu laden" }),
    ).toBeEnabled();
  });
  it("failed_codex_status_does_not_claim_loading_forever", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Windows Boot Manager");
    ipc.invoke.mockRejectedValueOnce({
      code: "io",
      message: "Codex-Konfiguration nicht lesbar.",
      details: null,
    });
    await user.click(screen.getByRole("button", { name: "Codex" }));
    await screen.findByText("Codex-Konfiguration nicht lesbar.");
    expect(
      screen.queryByText("Verbindungsstatus wird geladen …"),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Status prüfen" })).toBeEnabled();
  });
  it("unfinished_journal_remains_visible_and_restore_only_prepares", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Windows Boot Manager");
    await user.click(screen.getByRole("button", { name: "Sicherungen" }));
    expect(await screen.findByText("Unvollständiges Journal")).toBeVisible();
    await user.click(
      screen.getByRole("button", { name: "Wiederherstellung als Vorschau" }),
    );
    expect(
      await screen.findByRole("region", { name: "Änderungsvorschau" }),
    ).toBeVisible();
    expect(ipc.invoke).toHaveBeenCalledWith("prepare", {
      request: { kind: "restore", backup_id: "backup-1" },
    });
    expect(ipc.invoke.mock.calls.some((c) => c[0] === "apply")).toBe(false);
  });
  it("configured_codex_is_not_a_verified_connection", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Windows Boot Manager");
    await user.click(screen.getByRole("button", { name: "Codex" }));
    expect(await screen.findByText("Eingerichtet")).toBeVisible();
    expect(screen.getByText("Verbindung nicht bestätigt")).toBeVisible();
    expect(screen.getByText("C:\\BootPilot\\boot-mcp.exe")).toBeVisible();
  });
});
