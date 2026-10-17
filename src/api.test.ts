import { beforeEach, expect, it, vi } from "vitest";
const tauri = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
}));
vi.mock("@tauri-apps/api/core", () => tauri);
// Tests exercise the real boundary wrapper; only the desktop IPC is replaced.
beforeEach(() => {
  tauri.invoke.mockReset();
  tauri.isTauri.mockReturnValue(true);
});
it("browser_has_explicit_unavailable_state_and_never_returns_fake_data", async () => {
  const { api } = await import("./api");
  tauri.isTauri.mockReturnValue(false);
  await expect(api.getStatus()).rejects.toMatchObject({
    code: "desktop_unavailable",
    message: expect.stringContaining("Windows-App"),
  });
  expect(tauri.invoke).not.toHaveBeenCalled();
});
it("preserves_actionable_domain_error_details", async () => {
  const { api } = await import("./api");
  const error = {
    code: "conflict",
    message: "Konfiguration wurde geändert. Neue Vorschau erstellen.",
    details: { target: "grub_main" },
  };
  tauri.invoke.mockRejectedValue(error);
  await expect(api.apply("plan-id")).rejects.toEqual(error);
});
