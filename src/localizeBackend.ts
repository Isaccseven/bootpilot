import { i18n } from "./i18n";
import germanMessages from "./locales/backend-de.json";

// These resources describe native messages, not boot data. Match whole known
// messages only: paths, titles, configuration text, and unknown future messages
// must never undergo substring replacement or recursive translation.
const exactMessages = new Map(
  Object.entries(germanMessages)
    .filter(([, message]) => !message.includes("{{"))
    .map(([key, message]) => [message, key]),
);

const templates: {
  pattern: RegExp;
  key: keyof typeof germanMessages;
  parameter: string;
}[] = [
  {
    pattern: /^BootCurrent meldet den UEFI-Eintrag (Boot[0-9A-F]{4})\.$/,
    key: "bootCurrentObserved",
    parameter: "entry",
  },
  {
    pattern: /^(Secure Boot|BitLocker) ist laut Erfassung aktiviert\.$/,
    key: "securityEnabled",
    parameter: "name",
  },
  {
    pattern: /^(Secure Boot|BitLocker) ist laut Erfassung deaktiviert\.$/,
    key: "securityDisabled",
    parameter: "name",
  },
  {
    pattern: /^Der Status von (Secure Boot|BitLocker) ist unbekannt\.$/,
    key: "securityUnknown",
    parameter: "name",
  },
  {
    pattern: /^(Secure Boot|BitLocker)-Status mit geeigneten Leserechten erfassen\.$/,
    key: "securityCheck",
    parameter: "name",
  },
  {
    pattern: /^Direkter MCP-Test nicht bestätigt: ([\s\S]+)$/,
    key: "mcpProbeUnconfirmed",
    parameter: "reason",
  },
];

function translate(
  key: string,
  original: string,
  values: Record<string, string> = {},
): string {
  return i18n.t(key, {
    ...values,
    ns: "backend",
    defaultValue: original,
    // React escapes rendering. Do not interpret native payloads as HTML or as
    // nested i18next expressions, and preserve quotes/path separators exactly.
    interpolation: { escapeValue: false, skipOnVariables: true },
  });
}

/** Localize descriptive native text at render time; never mutate IPC objects. */
export function localizeBackendText(message: string): string {
  if (i18n.resolvedLanguage !== "en") return message;
  const key = exactMessages.get(message);
  if (key) return translate(key, message);
  for (const template of templates) {
    const match = template.pattern.exec(message);
    // JS `$` also matches before a final newline. Require the complete original
    // string so trailing bytes in unknown content are not discarded.
    if (match && match[0] === message) {
      const parameter = match[1];
      // integration.rs wraps a BootError.message here. Translate only a whole
      // known static message, never recurse into arbitrary nested text or JSON.
      const reasonKey = template.key === "mcpProbeUnconfirmed"
        ? exactMessages.get(parameter)
        : undefined;
      return translate(template.key, message, {
        [template.parameter]: reasonKey ? translate(reasonKey, parameter) : parameter,
      });
    }
  }
  return message;
}
