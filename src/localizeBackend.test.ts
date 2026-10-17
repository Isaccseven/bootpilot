import { beforeEach, describe, expect, it } from "vitest";
import { i18n } from "./i18n";
import { localizeBackendText } from "./localizeBackend";

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

describe("backend message presentation", () => {
  it("translates specific native errors and preview constraints without dropping detail", () => {
    expect(localizeBackendText("Die Wartezeit muss zwischen 1 und 300 Sekunden liegen."))
      .toBe("The timeout must be between 1 and 300 seconds.");
    expect(localizeBackendText("Kein gültiger Schreibnachweis für dieses Ziel; manuelle Prüfung erforderlich."))
      .toBe("No valid write record exists for this target; manual review is required.");
    expect(localizeBackendText("Alle vorhandenen Firmware-Einträge bleiben erhalten; nur ihre Reihenfolge ändert sich."))
      .toBe("All existing firmware entries are preserved; only their order changes.");
    expect(localizeBackendText("Die Grub2Win-Benutzersektion fehlt, ist mehrdeutig oder stimmt nicht bytegenau überein."))
      .toBe("The Grub2Win user section is missing, ambiguous, or does not match byte for byte.");
  });

  it("localizes narrowly defined findings while preserving their values", () => {
    expect(localizeBackendText("BootCurrent meldet den UEFI-Eintrag Boot00AF."))
      .toBe("BootCurrent reports UEFI entry Boot00AF.");
    expect(localizeBackendText("Secure Boot ist laut Erfassung aktiviert."))
      .toBe("Secure Boot is enabled according to the scan.");
    expect(localizeBackendText("BitLocker ist laut Erfassung deaktiviert."))
      .toBe("BitLocker is disabled according to the scan.");
    expect(localizeBackendText("Der Status von Secure Boot ist unbekannt."))
      .toBe("The status of Secure Boot is unknown.");
    expect(localizeBackendText("BitLocker-Status mit geeigneten Leserechten erfassen."))
      .toBe("Collect the BitLocker status with appropriate read permissions.");
  });

  it("keeps the direct MCP probe distinct from the current Codex connection", () => {
    expect(localizeBackendText("Direkter MCP-Test: initialize, tools/list und boot_get_status erfolgreich. Die Verbindung dieser Codex-Sitzung wurde dadurch nicht bestätigt."))
      .toBe("Direct MCP test: initialize, tools/list, and boot_get_status succeeded. This does not confirm the connection of this Codex session.");
  });

  it("preserves an interpolated MCP reason literally, including quotes and interpolation syntax", () => {
    const reason = '"C:\\Users\\Example & Test\\boot-mcp.exe" <native> {{name}} $t(planExpired)';
    expect(localizeBackendText(`Direkter MCP-Test nicht bestätigt: ${reason}`))
      .toBe(`Direct MCP test not confirmed: ${reason}`);
  });

  it.each([
    ["boot-mcp.exe ist nicht vorhanden oder nicht lesbar.", "boot-mcp.exe is missing or unreadable."],
    ["MCP antwortet nicht rechtzeitig.", "MCP did not respond in time."],
  ])("translates an exact known MCP failure reason: %s", (reason, translated) => {
    expect(localizeBackendText(`Direkter MCP-Test nicht bestätigt: ${reason}`))
      .toBe(`Direct MCP test not confirmed: ${translated}`);
  });

  it.each([
    "MCP antwortet nicht rechtzeitig. Additional native details: 0x1234",
    'C:\\Logs\\MCP antwortet nicht rechtzeitig.',
    "Direkter MCP-Test nicht bestätigt: MCP antwortet nicht rechtzeitig.",
    '{"message":"MCP antwortet nicht rechtzeitig."}',
  ])("does not partially or recursively translate an unknown MCP reason: %s", (reason) => {
    expect(localizeBackendText(`Direkter MCP-Test nicht bestätigt: ${reason}`))
      .toBe(`Direct MCP test not confirmed: ${reason}`);
  });

  it.each([
    "A future native error: 0x80041006",
    "BootCurrent meldet den UEFI-Eintrag Boot00AF.\n",
    "BootCurrent meldet den UEFI-Eintrag Boot00af.",
    "User entry ist laut Erfassung aktiviert.",
    'set default="Die Wartezeit muss zwischen 1 und 300 Sekunden liegen."\r\nset timeout=3\r\n',
    'C:\\GRUB\\Der Status von Secure Boot ist unbekannt.',
    '{"message":"Die Wartezeit muss zwischen 1 und 300 Sekunden liegen."}',
    "",
  ])("preserves unknown messages and raw content: %j", (message) => {
    expect(localizeBackendText(message)).toBe(message);
  });

  it("returns German messages byte for byte without reformatting", async () => {
    await i18n.changeLanguage("de");
    const messages = [
      "BootCurrent meldet den UEFI-Eintrag Boot00AF.",
      "Die Wartezeit muss zwischen 1 und 300 Sekunden liegen.",
      "Direkter MCP-Test nicht bestätigt: C:\\Apps\\boot-mcp.exe",
      "  Weitere Originaldaten\r\n",
    ];
    for (const message of messages) expect(localizeBackendText(message)).toBe(message);
  });
});
