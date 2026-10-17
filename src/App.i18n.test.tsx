import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import App from "./App";
import { snapshot, plan, backup } from "./test/fixtures";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, isTauri: () => true }));

beforeEach(() => {
  ipc.invoke.mockReset().mockImplementation((command: string) => {
    if (command === "get_status" || command === "refresh") return Promise.resolve(snapshot);
    if (command === "prepare") return Promise.resolve(plan());
    if (command === "list_backups") return Promise.resolve([backup]);
    return Promise.reject(new Error(command));
  });
});

it("switches language and document semantics without collecting boot data again", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("Windows Boot Manager");
  expect(screen.getByRole("combobox", { name: "Sprache" })).toHaveValue("de");
  await user.selectOptions(screen.getByRole("combobox", { name: "Sprache" }), "en");
  expect(screen.getByRole("navigation", { name: "Main navigation" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Overview" })).toBeVisible();
  expect(document.documentElement.lang).toBe("en");
  expect(localStorage.getItem("bootpilot.language.v1")).toBe("en");
  expect(within(screen.getByRole("group", { name: "Secure Boot" })).getByText("Unknown")).toBeVisible();
  expect(screen.getByText("Windows Boot Manager")).toBeVisible();
  expect(ipc.invoke.mock.calls.map(([command]) => command)).toEqual(["get_status"]);
});

it("preserves the draft and exact prepared plan when its language changes", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("Windows Boot Manager");
  await user.click(screen.getByRole("button", { name: "Bootmenü" }));
  const timeout = screen.getByRole("spinbutton", { name: "Wartezeit in Sekunden" });
  await user.clear(timeout);
  await user.type(timeout, "17");
  await user.click(screen.getByRole("button", { name: "Menüvorschau erstellen" }));
  await screen.findByRole("region", { name: "Änderungsvorschau" });
  await user.click(screen.getByRole("checkbox", { name: /Vorschau geprüft/ }));
  const requestBefore = structuredClone(ipc.invoke.mock.calls);
  await user.selectOptions(screen.getByRole("combobox", { name: "Sprache" }), "en");
  const preview = screen.getByRole("region", { name: "Change preview" });
  expect(within(preview).getByText("set timeout=3")).toBeVisible();
  expect(within(preview).getByText("set timeout=5")).toBeVisible();
  expect(within(preview).getByText("authoritative-snapshot-2")).toBeVisible();
  expect(screen.getByRole("spinbutton", { name: "Timeout in seconds" })).toHaveValue(17);
  expect(within(preview).getByRole("checkbox")).toBeChecked();
  expect(screen.getByRole("button", { name: "Apply changes" })).toBeEnabled();
  expect(ipc.invoke.mock.calls).toEqual(requestBefore);
  await user.selectOptions(screen.getByRole("combobox", { name: "Language" }), "de");
  expect(screen.getByRole("region", { name: "Änderungsvorschau" })).toBeVisible();
  expect(ipc.invoke.mock.calls).toEqual(requestBefore);
});

it("updates an in-flight operation label without restarting its IPC request", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("Windows Boot Manager");
  let resolve!: (value: typeof snapshot) => void;
  const pending = new Promise<typeof snapshot>((done) => { resolve = done; });
  ipc.invoke.mockReturnValueOnce(pending);
  await user.click(screen.getByRole("button", { name: "Aktualisieren" }));
  expect(screen.getByRole("status")).toHaveTextContent("System erfassen");
  await user.selectOptions(screen.getByRole("combobox", { name: "Sprache" }), "en");
  expect(screen.getByRole("status")).toHaveTextContent("Scanning system");
  expect(ipc.invoke.mock.calls.map(([command]) => command)).toEqual(["get_status", "refresh"]);
  await act(async () => { resolve(snapshot); await pending; });
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
});
