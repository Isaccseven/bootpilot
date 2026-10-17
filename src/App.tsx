import { useEffect, useRef, useState } from "react";
import { api, asBootError } from "./api";
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
  Source,
  Target,
} from "./types";
import "./styles.css";

type View = "overview" | "menu" | "backups" | "codex";
const views: {
  id: View;
  label: string;
  icon: string;
  title: string;
  subtitle: string;
}[] = [
  {
    id: "overview",
    label: "Übersicht",
    icon: "◫",
    title: "Dein System im Blick.",
    subtitle:
      "Vorhandene Startoptionen und der Stand deiner Bootkonfiguration.",
  },
  {
    id: "menu",
    label: "Bootmenü",
    icon: "☷",
    title: "Der nächste Start beginnt hier.",
    subtitle: "Änderungen vorbereiten, genau prüfen und anschließend anwenden.",
  },
  {
    id: "backups",
    label: "Sicherungen",
    icon: "↶",
    title: "Jede Änderung nachvollziehbar.",
    subtitle:
      "Gesicherte Ziele, verifizierte Schritte und gezielte Wiederherstellung.",
  },
  {
    id: "codex",
    label: "Codex",
    icon: "⌘",
    title: "Dein Bootmenü. Mit Codex.",
    subtitle: "Dieselben geprüften Funktionen, direkt aus deinem Codex-Chat.",
  },
];
const sourceNames: Record<Source, string> = {
  firmware: "Firmware",
  bcd: "Windows BCD",
  boot_current: "Aktueller Start",
  secure_boot: "Secure Boot",
  bitlocker: "BitLocker",
  grub: "GRUB",
  linux: "Linux-Erkennung",
  volumes: "Volumes",
};
const targetNames: Record<Target, string> = {
  firmware_order: "UEFI-Bootreihenfolge",
  grub_main: "GRUB-Hauptkonfiguration",
  grub_user: "GRUB-Benutzersektion",
};
const stateNames = {
  complete: "Vollständig",
  partial: "Unvollständig",
  unavailable: "Nicht verfügbar",
  unknown: "Unbekannt",
};
const kindNames = {
  windows: "Windows",
  linux: "Installiertes Linux",
  installer: "Installer / Live-System",
  other: "Weiterer Eintrag",
};
const outcomeNames = {
  applied: "Konfiguration geprüft",
  partial: "Teilweise ausgeführt",
  rolled_back: "Änderung zurückgenommen",
};
const phaseNames = {
  intent: "Vorbereitet",
  written: "Geschrieben",
  verified: "Geprüft",
  rollback_intent: "Rücknahme vorbereitet",
  rolled_back: "Zurückgenommen",
  failed: "Fehlgeschlagen",
};
function date(value: string) {
  const stamp = new Date(value);
  return Number.isFinite(stamp.getTime())
    ? stamp.toLocaleString("de-DE", {
        dateStyle: "medium",
        timeStyle: "medium",
      })
    : "Zeitpunkt unbekannt";
}
function requestName(request: PlanRequest) {
  return {
    order: "Bootreihenfolge",
    menu: "Standard & Wartezeit",
    cachyos: "CachyOS-Eintrag",
    restore: "Gezielte Wiederherstellung",
  }[request.kind];
}

function ErrorNotice({ error }: { error: BootError }) {
  return (
    <div className="notice danger" role="alert">
      <strong>{error.message}</strong>
      {error.details != null && (
        <details>
          <summary>Details anzeigen</summary>
          <pre>
            {typeof error.details === "string"
              ? error.details
              : JSON.stringify(error.details, null, 2)}
          </pre>
        </details>
      )}
      <small>Fehlercode: {error.code}</small>
    </div>
  );
}
function Empty({
  title,
  children,
}: {
  title: string;
  children?: React.ReactNode;
}) {
  return (
    <div className="empty">
      <span aria-hidden="true">◌</span>
      <h3>{title}</h3>
      {children && <p>{children}</p>}
    </div>
  );
}
function Report({ report }: { report: ApplyReport }) {
  return (
    <section
      className={`panel result ${report.outcome === "partial" ? "warning-border" : ""}`}
      aria-label="Änderungsergebnis"
    >
      <div className="section-heading">
        <div>
          <p className="eyebrow">ERGEBNIS</p>
          <h2>{outcomeNames[report.outcome]}</h2>
        </div>
        <span className="badge">
          {report.boot_tested ? "Boot getestet" : "Boot nicht getestet"}
        </span>
      </div>
      <p>
        {report.outcome === "applied"
          ? "Die angegebenen Konfigurationsschritte wurden nach dem Schreiben verifiziert."
          : report.outcome === "partial"
            ? "Es verbleiben Abweichungen. Prüfe das Protokoll und die zugehörige Sicherung."
            : "Die betroffenen Ziele wurden zurückgenommen. Einzelheiten stehen im Protokoll."}
      </p>
      <dl className="metadata">
        <div>
          <dt>Plan</dt>
          <dd>{report.plan_id}</dd>
        </div>
        <div>
          <dt>Sicherung</dt>
          <dd>{report.backup_id ?? "Keine Sicherung gemeldet"}</dd>
        </div>
      </dl>
      {report.verified_steps.length > 0 && (
        <ul className="plain-list">
          {report.verified_steps.map((step) => (
            <li key={`${step.sequence}-${step.phase}`}>
              {targetNames[step.target]} · {phaseNames[step.phase]}
              {step.detail && ` · ${step.detail}`}
            </li>
          ))}
        </ul>
      )}
      {report.remaining_deviations.length > 0 && (
        <div className="notice warning">
          <h3>Verbleibende Abweichungen</h3>
          <ul>
            {report.remaining_deviations.map((deviation, i) => (
              <li key={i}>
                <strong>{targetNames[deviation.target]}:</strong>{" "}
                {deviation.description}
              </li>
            ))}
          </ul>
        </div>
      )}
      {!report.boot_tested && (
        <p className="muted">
          Ein erfolgreicher Start lässt sich erst durch einen separat
          durchgeführten Boot-Test bestätigen.
        </p>
      )}
    </section>
  );
}

function Preview({
  plan,
  blocked,
  busy,
  onApply,
  onDismiss,
}: {
  plan: ChangePlan;
  blocked: boolean;
  busy: boolean;
  onApply: () => void;
  onDismiss: () => void;
}) {
  const [checked, setChecked] = useState(false);
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  const expires = Date.parse(plan.expires_at);
  const expired = !Number.isFinite(expires) || now >= expires;
  const unavailable =
    blocked || expired || plan.state !== "ready" || plan.diffs.length === 0;
  return (
    <section className="panel preview" aria-label="Änderungsvorschau">
      <div className="section-heading">
        <div>
          <p className="eyebrow">VOR DEM ANWENDEN</p>
          <h2>Änderungsvorschau</h2>
        </div>
        <span className="badge blue">{requestName(plan.request)}</span>
      </div>
      <p>
        Prüfe alle betroffenen Ziele. Vor dem Schreiben erstellt BootPilot eine
        Sicherung und prüft, ob der Ausgangszustand noch übereinstimmt.
      </p>
      <dl className="metadata">
        <div>
          <dt>Plan-ID</dt>
          <dd>{plan.id}</dd>
        </div>
        <div>
          <dt>Gültig bis</dt>
          <dd>{date(plan.expires_at)}</dd>
        </div>
        <div>
          <dt>Erfassung</dt>
          <dd>{plan.snapshot_id}</dd>
        </div>
      </dl>
      {plan.diffs.map((diff, index) => (
        <article className="diff" key={`${diff.target}-${index}`}>
          <h3>{targetNames[diff.target]}</h3>
          <p>{diff.description}</p>
          <div className="diff-grid">
            <div>
              <h4>Vorher</h4>
              <pre>{diff.before || "(leer)"}</pre>
            </div>
            <div>
              <h4>Nachher</h4>
              <pre>{diff.after || "(leer)"}</pre>
            </div>
          </div>
        </article>
      ))}
      {plan.preconditions.length > 0 && (
        <details className="checksums">
          <summary>
            Geprüfte Vorbedingungen ({plan.preconditions.length})
          </summary>
          {plan.preconditions.map((condition) => (
            <p key={condition.target}>
              {targetNames[condition.target]}
              <code>{condition.expected_checksum}</code>
            </p>
          ))}
        </details>
      )}
      {expired && (
        <p className="notice warning" role="status">
          Plan abgelaufen. Erstelle eine neue Vorschau.
        </p>
      )}
      {blocked && (
        <p className="notice warning">
          Es liegt keine aktuelle Erfassung vor. Aktualisiere und erstelle eine
          neue Vorschau.
        </p>
      )}
      {plan.state !== "ready" && (
        <p className="notice warning">
          Dieser Plan wurde bereits begonnen oder abgeschlossen.
        </p>
      )}
      {plan.diffs.length === 0 && (
        <p className="notice warning">
          Dieser Plan enthält keine sichtbaren Änderungen und kann hier nicht
          angewendet werden.
        </p>
      )}
      <label className="confirmation">
        <input
          type="checkbox"
          checked={checked}
          disabled={unavailable || busy}
          onChange={(event) => setChecked(event.target.checked)}
        />
        Vorschau geprüft – diese Änderungen anwenden
      </label>
      <div className="actions">
        <button
          className="primary"
          disabled={!checked || unavailable || busy}
          onClick={onApply}
        >
          {busy ? "Wird angewendet …" : "Änderungen anwenden"}
        </button>
        <button disabled={busy} onClick={onDismiss}>
          Vorschau verwerfen
        </button>
        <span className="muted">Kann eine Administratorabfrage öffnen.</span>
      </div>
    </section>
  );
}

function Overview({
  snapshot,
  findings,
  onDiagnose,
  busy,
}: {
  snapshot: Snapshot;
  findings: Finding[] | null;
  onDiagnose: () => void;
  busy: boolean;
}) {
  const current = snapshot.boot_current;
  return (
    <>
      <div className="summary-grid">
        <section
          className="panel summary-card"
          role="group"
          aria-label="Secure Boot"
        >
          <span className="small-icon">◇</span>
          <h2>Secure Boot</h2>
          <strong>
            {snapshot.secure_boot === null
              ? "Unbekannt"
              : snapshot.secure_boot
                ? "Aktiviert"
                : "Deaktiviert"}
          </strong>
          <p>Quelle: {stateNames[snapshot.sources.secure_boot]}</p>
        </section>
        <section
          className="panel summary-card"
          role="group"
          aria-label="BitLocker"
        >
          <span className="small-icon">▣</span>
          <h2>BitLocker</h2>
          <strong>
            {snapshot.bitlocker === null
              ? "Unbekannt"
              : snapshot.bitlocker
                ? "Schutz aktiviert"
                : "Schutz deaktiviert"}
          </strong>
          <p>Windows-Systemvolume · {stateNames[snapshot.sources.bitlocker]}</p>
        </section>
        <section className="panel summary-card">
          <span className="small-icon">↗</span>
          <h2>Aktueller Firmware-Start</h2>
          <strong>
            {current
              ? `Boot${current.uefi_number.toString(16).toUpperCase().padStart(4, "0")}`
              : "Unbekannt"}
          </strong>
          <p>
            {current?.description ?? "Keine bestätigte Zuordnung verfügbar"}
          </p>
        </section>
      </div>
      <div className="overview-grid">
        <section className="panel">
          <div className="section-heading">
            <div>
              <p className="eyebrow">UEFI</p>
              <h2>Bootreihenfolge</h2>
            </div>
            <span className="badge">
              {stateNames[snapshot.sources.firmware]}
            </span>
          </div>
          {snapshot.firmware_order.length ? (
            <ol className="entry-list">
              {snapshot.firmware_order.map((id, index) => {
                const entry = snapshot.firmware_entries.find(
                  (item) => item.id === id,
                );
                return (
                  <li key={id}>
                    <span className="entry-number">{index + 1}</span>
                    <div>
                      <strong>
                        {entry?.title ?? "Unbekannter Firmware-Eintrag"}
                      </strong>
                      <small>{entry?.loader_path ?? id}</small>
                    </div>
                  </li>
                );
              })}
            </ol>
          ) : (
            <Empty title="Keine verifizierte Reihenfolge">
              Ein leerer Befund bestätigt keine fehlenden Betriebssysteme.
            </Empty>
          )}
          {snapshot.boot_next !== null && (
            <p className="notice info">
              Einmaliger nächster Start: Boot
              {snapshot.boot_next.toString(16).toUpperCase().padStart(4, "0")}
            </p>
          )}
        </section>
        <section className="panel">
          <div className="section-heading">
            <div>
              <p className="eyebrow">GRUB</p>
              <h2>Erkannte Menüeinträge</h2>
            </div>
          </div>
          {snapshot.grub?.entries.length ? (
            <ul className="entry-list">
              {snapshot.grub.entries.map((entry) => (
                <li key={entry.id}>
                  <span className={`os-icon ${entry.kind}`} aria-hidden="true">
                    {entry.kind === "windows"
                      ? "⊞"
                      : entry.kind === "installer"
                        ? "◎"
                        : "◈"}
                  </span>
                  <div>
                    <strong>{entry.title}</strong>
                    <small>{kindNames[entry.kind]}</small>
                  </div>
                  {snapshot.grub?.default_entry_id === entry.id && (
                    <span className="badge">Standard</span>
                  )}
                </li>
              ))}
            </ul>
          ) : (
            <Empty title="GRUB-Menü nicht verfügbar">
              Erst eine vollständige Erfassung ermöglicht eine sichere Aussage.
            </Empty>
          )}
          {!snapshot.linux_candidates.some(
            (candidate) => candidate.validated,
          ) && (
            <p className="muted">
              Keine validierte Linux-Installation für einen direkten neuen
              Eintrag erkannt. Installer sind keine installierten Linux-Systeme.
            </p>
          )}
        </section>
      </div>
      <section className="panel">
        <div className="section-heading">
          <div>
            <p className="eyebrow">DATENQUALITÄT</p>
            <h2>Erfassung & offene Prüfungen</h2>
          </div>
          <button onClick={onDiagnose} disabled={busy}>
            Diagnose anzeigen
          </button>
        </div>
        <div className="source-grid">
          {(Object.keys(sourceNames) as Source[]).map((source) => (
            <div key={source}>
              <span>{sourceNames[source]}</span>
              <span
                className={`state ${snapshot.sources[source] === "complete" ? "complete" : ""}`}
              >
                {stateNames[snapshot.sources[source]]}
              </span>
            </div>
          ))}
        </div>
        {snapshot.source_errors.map((item, i) => (
          <div className="source-error" key={i}>
            <strong>{sourceNames[item.source]}</strong>
            <ErrorNotice error={item.error} />
          </div>
        ))}
        {findings !== null &&
          (findings.length ? (
            <ul className="findings">
              {findings.map((finding, i) => (
                <li key={`${finding.code}-${i}`}>
                  <span className="badge">
                    {finding.certainty === "evidence" ? "Belegt" : "Hypothese"}
                  </span>
                  <strong>{finding.statement}</strong>
                  {finding.open_check && (
                    <p>Offene Prüfung: {finding.open_check}</p>
                  )}
                  {finding.evidence.length > 0 && (
                    <small>
                      {finding.evidence
                        .map(
                          (evidence) =>
                            `${sourceNames[evidence.source]}: ${evidence.reference}`,
                        )
                        .join(" · ")}
                    </small>
                  )}
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted">
              Die Diagnose meldet keine zusätzlichen Befunde. Dies bestätigt
              keinen erfolgreichen Boot-Test.
            </p>
          ))}
      </section>
      {(snapshot.loader_info ||
        snapshot.loader_entry_selected ||
        snapshot.volumes.length > 0) && (
        <details className="panel technical">
          <summary>Weitere erfasste Systemdaten</summary>
          {snapshot.loader_info && <p>Loader: {snapshot.loader_info}</p>}
          {snapshot.loader_entry_selected && (
            <p>Ausgewählter Loader-Eintrag: {snapshot.loader_entry_selected}</p>
          )}
          {snapshot.volumes.map((volume, i) => (
            <p key={volume.volume_id ?? i}>
              {volume.paths.join(", ") ||
                volume.volume_id ||
                "Volume ohne Zuordnung"}{" "}
              · {volume.filesystem ?? "Dateisystem unbekannt"}
              {volume.size_bytes !== null &&
                ` · ${(volume.size_bytes / 1024 ** 3).toLocaleString("de-DE", { maximumFractionDigits: 1 })} GiB`}
            </p>
          ))}
        </details>
      )}
    </>
  );
}

function BootMenu({
  snapshot,
  blocked,
  busy,
  onPrepare,
  invalidate,
}: {
  snapshot: Snapshot;
  blocked: boolean;
  busy: boolean;
  onPrepare: (request: PlanRequest) => void;
  invalidate: () => void;
}) {
  const [defaultId, setDefaultId] = useState(
    snapshot.grub?.default_entry_id ?? "",
  );
  const [timeout, setTimeout] = useState(
    snapshot.grub?.timeout_seconds?.toString() ?? "",
  );
  const [order, setOrder] = useState(snapshot.firmware_order);
  const candidates = snapshot.linux_candidates.filter(
    (candidate) => candidate.validated,
  );
  const [candidateId, setCandidateId] = useState(candidates[0]?.id ?? "");
  const [title, setTitle] = useState("CachyOS");
  const grub = snapshot.grub;
  const canMenu =
    !blocked &&
    !busy &&
    grub?.write_supported &&
    snapshot.sources.grub === "complete";
  const canOrder =
    !blocked && !busy && snapshot.sources.firmware === "complete";
  const move = (index: number, offset: number) => {
    const next = [...order];
    [next[index], next[index + offset]] = [next[index + offset], next[index]];
    setOrder(next);
    invalidate();
  };
  return (
    <>
      <section className="panel">
        <div className="section-heading">
          <div>
            <p className="eyebrow">GRUB-BOOTMENÜ</p>
            <h2>Standard & Wartezeit</h2>
          </div>
          <span className="badge">
            {grub?.write_supported
              ? "Unterstützte Konfiguration"
              : "Nur lesbar"}
          </span>
        </div>
        {grub ? (
          <form
            onSubmit={(event) => {
              event.preventDefault();
              onPrepare({
                kind: "menu",
                default_entry_id: defaultId,
                timeout_seconds: Number(timeout),
              });
            }}
          >
            <div className="form-grid">
              <label>
                Standard-Eintrag
                <select
                  value={defaultId}
                  onChange={(event) => {
                    setDefaultId(event.target.value);
                    invalidate();
                  }}
                  disabled={!canMenu}
                >
                  <option value="" disabled>
                    Eintrag wählen
                  </option>
                  {grub.entries.map((entry) => (
                    <option key={entry.id} value={entry.id}>
                      {entry.title} · {kindNames[entry.kind]}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Wartezeit in Sekunden
                <input
                  type="number"
                  min="1"
                  max="300"
                  step="1"
                  value={timeout}
                  onChange={(event) => {
                    setTimeout(event.target.value);
                    invalidate();
                  }}
                  disabled={!canMenu}
                  required
                />
              </label>
            </div>
            {!grub.write_supported && (
              <p className="notice warning">
                {grub.write_block_reason ??
                  "Diese GRUB-Konfiguration wird zum Schreiben nicht unterstützt."}
              </p>
            )}
            <button
              type="submit"
              disabled={
                !canMenu ||
                !defaultId ||
                timeout === "" ||
                !Number.isInteger(Number(timeout)) ||
                Number(timeout) < 1 ||
                Number(timeout) > 300
              }
            >
              Menüvorschau erstellen
            </button>
          </form>
        ) : (
          <Empty title="Keine GRUB-Konfiguration verfügbar">
            Aktualisiere die Erfassung, um unterstützte Einstellungen zu sehen.
          </Empty>
        )}
      </section>
      <section className="panel">
        <div className="section-heading">
          <div>
            <p className="eyebrow">FIRMWARE</p>
            <h2>UEFI-Bootreihenfolge</h2>
          </div>
        </div>
        <p className="muted">
          Die Reihenfolge ist unabhängig von der Standardauswahl im GRUB-Menü.
        </p>
        {order.length ? (
          <>
            <ol className="entry-list reorder">
              {order.map((id, index) => {
                const name =
                  snapshot.firmware_entries.find((entry) => entry.id === id)
                    ?.title ?? id;
                return (
                  <li key={id}>
                    <span className="entry-number">{index + 1}</span>
                    <strong>{name}</strong>
                    <div className="move-buttons">
                      <button
                        aria-label={`${name} nach oben`}
                        disabled={!canOrder || index === 0}
                        onClick={() => move(index, -1)}
                      >
                        ↑
                      </button>
                      <button
                        aria-label={`${name} nach unten`}
                        disabled={!canOrder || index === order.length - 1}
                        onClick={() => move(index, 1)}
                      >
                        ↓
                      </button>
                    </div>
                  </li>
                );
              })}
            </ol>
            <button
              disabled={!canOrder}
              onClick={() => onPrepare({ kind: "order", entry_ids: order })}
            >
              Reihenfolge als Vorschau
            </button>
          </>
        ) : (
          <Empty title="Reihenfolge nicht verfügbar" />
        )}
        {!canOrder && !busy && (
          <p className="muted">
            Eine vollständige aktuelle Firmware-Erfassung ist erforderlich.
          </p>
        )}
      </section>
      <section className="panel">
        <div className="section-heading">
          <div>
            <p className="eyebrow">VALIDIERTE INSTALLATION</p>
            <h2>CachyOS direkt hinzufügen</h2>
          </div>
        </div>
        {candidates.length ? (
          <form
            onSubmit={(event) => {
              event.preventDefault();
              onPrepare({
                kind: "cachyos",
                candidate_id: candidateId,
                title: title.trim(),
              });
            }}
          >
            <div className="form-grid">
              <label>
                Erkannte Installation
                <select
                  value={candidateId}
                  disabled={blocked || busy}
                  onChange={(event) => {
                    setCandidateId(event.target.value);
                    invalidate();
                  }}
                >
                  {candidates.map((candidate) => (
                    <option key={candidate.id} value={candidate.id}>
                      {candidate.title}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Menütitel
                <input
                  maxLength={120}
                  value={title}
                  disabled={blocked || busy}
                  onChange={(event) => {
                    setTitle(event.target.value);
                    invalidate();
                  }}
                  required
                />
              </label>
            </div>
            {candidates
              .filter((candidate) => candidate.id === candidateId)
              .map((candidate) => (
                <p key={candidate.id} className="muted">
                  Geprüfter Kernel: <code>{candidate.kernel_path}</code> ·
                  Dateisystem: <code>{candidate.filesystem_uuid}</code>
                </p>
              ))}
            <button
              disabled={
                !canMenu ||
                !["complete", "partial"].includes(snapshot.sources.linux) ||
                !title.trim()
              }
              type="submit"
            >
              CachyOS-Vorschau erstellen
            </button>
          </form>
        ) : (
          <Empty title="Keine validierte Installation verfügbar">
            Ein vorhandener ISO- oder Live-Installer wird nicht als installierte
            Linux-Installation verwendet.
          </Empty>
        )}
      </section>
    </>
  );
}

function Backups({
  backups,
  busy,
  onReload,
  onRestore,
}: {
  backups: BackupRecord[] | null;
  busy: boolean;
  onReload: () => void;
  onRestore: (id: string) => void;
}) {
  return (
    <section className="panel">
      <div className="section-heading">
        <div>
          <p className="eyebrow">ÄNDERUNGSPROTOKOLL</p>
          <h2>Deine Sicherungen</h2>
        </div>
        <button onClick={onReload} disabled={busy}>
          Sicherungen neu laden
        </button>
      </div>
      <p className="muted">
        Eine Wiederherstellung wird zuerst als konkreter Plan geprüft. Sie
        betrifft nur die Ziele dieser eigenen Änderung.
      </p>
      {backups === null ? (
        <p role="status">
          {busy
            ? "Sicherungen werden geladen …"
            : "Sicherungen sind noch nicht geladen. Wähle „Sicherungen neu laden“."}
        </p>
      ) : backups.length === 0 ? (
        <Empty title="Noch keine Sicherungen vorhanden">
          Sobald eine Änderung beginnt, erscheint ihre Sicherung mit dem
          Protokoll hier.
        </Empty>
      ) : (
        <div className="backup-list">
          {backups.map((backup) => (
            <article key={backup.id} className="backup">
              <div className="section-heading">
                <div>
                  <h3>{requestName(backup.request)}</h3>
                  <p>{date(backup.created_at)}</p>
                </div>
                <span
                  className={`badge ${backup.outcome === null ? "amber" : ""}`}
                >
                  {backup.outcome
                    ? outcomeNames[backup.outcome.outcome]
                    : "Unvollständiges Journal"}
                </span>
              </div>
              <dl className="metadata">
                <div>
                  <dt>Sicherungs-ID</dt>
                  <dd>{backup.id}</dd>
                </div>
                <div>
                  <dt>Plan</dt>
                  <dd>{backup.plan_id}</dd>
                </div>
              </dl>
              {backup.outcome === null && (
                <p className="notice warning">
                  Diese Änderung wurde noch nicht als abgeschlossen
                  protokolliert. Der Zustand muss vor der Wiederherstellung
                  geprüft werden.
                </p>
              )}
              <details>
                <summary>Gesicherte Ziele & Protokoll</summary>
                {backup.targets.map((target) => (
                  <div className="backup-target" key={target.target}>
                    <strong>{targetNames[target.target]}</strong>
                    <p>
                      Original: <code>{target.original_checksum}</code>
                    </p>
                    <p>
                      Angewendet: <code>{target.applied_checksum}</code>
                    </p>
                  </div>
                ))}
                {backup.journal.length ? (
                  <ol>
                    {backup.journal.map((step) => (
                      <li key={`${step.sequence}-${step.phase}`}>
                        {date(step.at)} · {targetNames[step.target]} ·{" "}
                        {phaseNames[step.phase]}
                        {step.detail && ` · ${step.detail}`}
                      </li>
                    ))}
                  </ol>
                ) : (
                  <p className="muted">
                    Keine verifizierten Schritte protokolliert.
                  </p>
                )}
              </details>
              {backup.outcome?.remaining_deviations.map((item, i) => (
                <p key={i} className="notice warning">
                  {targetNames[item.target]}: {item.description}
                </p>
              ))}
              <button disabled={busy} onClick={() => onRestore(backup.id)}>
                Wiederherstellung als Vorschau
              </button>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}

function Codex({
  status,
  result,
  busy,
  onRegister,
  onReload,
}: {
  status: IntegrationStatus | null;
  result: RegistrationResult | null;
  busy: boolean;
  onRegister: () => void;
  onReload: () => void;
}) {
  return (
    <>
      <section className="panel">
        <div className="section-heading">
          <div>
            <p className="eyebrow">LOKALE MCP-ANBINDUNG</p>
            <h2>BootPilot in Codex</h2>
          </div>
          <button disabled={busy} onClick={onReload}>
            Status prüfen
          </button>
        </div>
        {status ? (
          <>
            <div className="integration-status">
              <div>
                <span className="status-dot" />
                <strong>
                  {status.configured
                    ? "Eingerichtet"
                    : "Noch nicht eingerichtet"}
                </strong>
              </div>
              <span
                className={`badge ${status.connection_verified ? "blue" : ""}`}
              >
                {status.connection_verified
                  ? "Verbindung gemessen und bestätigt"
                  : "Verbindung nicht bestätigt"}
              </span>
            </div>
            <dl className="metadata">
              <div>
                <dt>MCP-Server</dt>
                <dd>{status.mcp_path ?? "Kein Pfad eingerichtet"}</dd>
              </div>
            </dl>
            <p className="muted">{status.details}</p>
          </>
        ) : (
          <p role="status">
            {busy
              ? "Verbindungsstatus wird geladen …"
              : "Verbindungsstatus nicht verfügbar. Wähle „Status prüfen“."}
          </p>
        )}
        <button
          className="primary"
          disabled={busy || status === null}
          onClick={onRegister}
        >
          {status?.configured
            ? "Codex-Einrichtung erneut prüfen"
            : "Für Codex einrichten"}
        </button>
        <p className="muted">
          Die Einrichtung sichert die Codex-Konfiguration und erhält vorhandene
          MCP-Server. Ein zusätzlicher API-Schlüssel ist nicht nötig.
        </p>
        {result && (
          <div className="notice info" role="status">
            <strong>Einrichtung gespeichert</strong>
            <p>
              Konfiguration: <code>{result.config_path}</code>
            </p>
            <p>
              Sicherung: <code>{result.backup_path}</code>
            </p>
            {result.restart_required && (
              <p>
                Starte Codex vollständig neu. Erst ein gemessener
                Verbindungstest bestätigt die Verbindung.
              </p>
            )}
          </div>
        )}
      </section>
      <section className="panel">
        <p className="eyebrow">BEISPIELE FÜR DEINEN CHAT</p>
        <h2>Ein Auftrag genügt.</h2>
        <ul className="examples">
          <li>„Zeige meine Bootoptionen und offene Prüfungen.“</li>
          <li>„Bereite CachyOS als Standard vor.“</li>
          <li>„Zeige die Vorschau für die letzte Wiederherstellung.“</li>
        </ul>
        <p className="muted">
          Auch in Codex erfordert jede Änderung einen geprüften Plan. BootPilot
          startet den PC nicht automatisch neu.
        </p>
      </section>
    </>
  );
}

export default function App() {
  const [view, setView] = useState<View>("overview");
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [stale, setStale] = useState(false);
  const [error, setError] = useState<BootError | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [findings, setFindings] = useState<Finding[] | null>(null);
  const [plan, setPlan] = useState<ChangePlan | null>(null);
  const [report, setReport] = useState<ApplyReport | null>(null);
  const [backups, setBackups] = useState<BackupRecord[] | null>(null);
  const [integration, setIntegration] = useState<IntegrationStatus | null>(
    null,
  );
  const [registration, setRegistration] = useState<RegistrationResult | null>(
    null,
  );
  // A scan UUID identifies provenance, not target state. Invalidate asynchronous
  // previews by the UI interaction that requested them; core checks real state.
  const planGeneration = useRef(0);
  useEffect(() => {
    let active = true;
    api
      .getStatus()
      .then((value) => {
        if (active) setSnapshot(value);
      })
      .catch((value) => {
        if (active) setError(asBootError(value));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
      planGeneration.current += 1;
    };
  }, []);
  function invalidatePlan() {
    planGeneration.current += 1;
    setPlan(null);
  }
  async function run(label: string, action: () => Promise<void>) {
    if (busy) return;
    setBusy(label);
    setError(null);
    try {
      await action();
    } catch (value) {
      setError(asBootError(value));
    } finally {
      setBusy(null);
    }
  }
  function navigate(next: View) {
    if (next !== view) invalidatePlan();
    setView(next);
    if (next === "backups" && backups === null && !busy)
      void run("Sicherungen laden", async () =>
        setBackups(await api.listBackups()),
      );
    if (next === "codex" && integration === null && !busy)
      void run("Codex-Status laden", async () =>
        setIntegration(await api.getIntegrationStatus()),
      );
  }
  function refresh() {
    if (busy) return;
    invalidatePlan();
    void run("System erfassen", async () => {
      try {
        const fresh = await api.refresh();
        setSnapshot(fresh);
        setStale(false);
        setFindings(null);
      } catch (value) {
        setStale(true);
        throw value;
      }
    });
  }
  function prepare(request: PlanRequest) {
    if (busy) return;
    const generation = ++planGeneration.current;
    void run("Vorschau vorbereiten", async () => {
      setPlan(null);
      setReport(null);
      const prepared = await api.prepare(request);
      if (generation === planGeneration.current) setPlan(prepared);
    });
  }
  function apply() {
    if (
      !plan ||
      stale ||
      !snapshot ||
      plan.state !== "ready" ||
      Date.now() >= Date.parse(plan.expires_at)
    )
      return;
    const concretePlan = plan;
    void run("Änderungen anwenden", async () => {
      try {
        setReport(await api.apply(concretePlan.id));
        setStale(true);
        setBackups(null);
      } finally {
        invalidatePlan();
        setStale(true);
      }
    });
  }
  const selectedView = views.find((item) => item.id === view)!;
  const incomplete =
    snapshot &&
    Object.values(snapshot.sources).some((state) => state !== "complete");
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <svg viewBox="0 0 36 36" aria-hidden="true">
            <rect
              x="1"
              y="1"
              width="34"
              height="34"
              rx="10"
              fill="currentColor"
            />
            <path
              d="M11 25V11h8a4 4 0 0 1 0 8h-8m6 0 7 6"
              stroke="white"
              strokeWidth="2.5"
              fill="none"
              strokeLinejoin="round"
            />
          </svg>
          <span>
            BootPilot<small>STARTKLAR. MIT ÜBERBLICK.</small>
          </span>
        </div>
        <nav aria-label="Hauptnavigation">
          {views.map((item) => (
            <button
              key={item.id}
              className={view === item.id ? "active" : ""}
              aria-current={view === item.id ? "page" : undefined}
              onClick={() => navigate(item.id)}
            >
              <span aria-hidden="true">{item.icon}</span>
              {item.label}
            </button>
          ))}
        </nav>
        <div className="sidebar-note">
          <span className="status-dot" />
          <strong>Lokal auf deinem PC</strong>
          <p>
            Geprüfte Vorschau.
            <br />
            Nachvollziehbare Änderungen.
          </p>
          <small>BootPilot · Version 0.1</small>
        </div>
      </aside>
      <div className="workspace">
        <header className="topbar">
          <span>Deine Startkonfiguration</span>
          <span className="local-label">
            <span className="status-dot" />
            Windows · lokal
          </span>
        </header>
        <main id="main-content">
          <div className="page-heading">
            <div>
              <p className="eyebrow">{selectedView.label.toUpperCase()}</p>
              <h1>{selectedView.title}</h1>
              <p>{selectedView.subtitle}</p>
            </div>
            <button
              className="refresh"
              disabled={!!busy || loading}
              onClick={refresh}
            >
              <span aria-hidden="true">↻</span>Aktualisieren
            </button>
          </div>
          <div className="snapshot-bar">
            <span
              className={`status-dot ${stale || incomplete ? "amber" : ""}`}
            />
            <span>
              {loading
                ? "Gespeicherten Status laden …"
                : snapshot
                  ? `${stale ? "Veraltet · " : ""}Letzte Erfassung: ${date(snapshot.captured_at)}`
                  : "Noch keine Systemerfassung vorhanden"}
            </span>
            {snapshot && (
              <span className="badge">
                {incomplete ? "Teilweise erfasst" : "Vollständig erfasst"}
              </span>
            )}
          </div>
          {busy && (
            <div className="working" role="status">
              <span className="spinner" />
              {busy} …{" "}
              <span className="muted">Die Navigation bleibt verfügbar.</span>
            </div>
          )}
          {error && <ErrorNotice error={error} />}
          {stale && snapshot && (
            <div className="notice warning">
              Die letzte Erfassung bleibt sichtbar. Aktualisiere den Status,
              bevor du eine weitere Änderung vorbereitest.
            </div>
          )}
          {!loading &&
            !snapshot &&
            (view === "overview" || view === "menu") && (
              <section className="panel">
                <Empty title="Erst erfassen, dann entscheiden.">
                  Mit „Aktualisieren“ liest BootPilot die vorhandene
                  Bootkonfiguration. Für geschützte Abfragen kann Windows nach
                  Administratorrechten fragen.
                </Empty>
              </section>
            )}
          {snapshot && view === "overview" && (
            <Overview
              snapshot={snapshot}
              findings={findings}
              busy={!!busy}
              onDiagnose={() =>
                void run("Diagnose laden", async () =>
                  setFindings(await api.diagnose()),
                )
              }
            />
          )}
          {snapshot && view === "menu" && (
            <BootMenu
              key={snapshot.id}
              snapshot={snapshot}
              blocked={stale}
              busy={!!busy}
              onPrepare={prepare}
              invalidate={invalidatePlan}
            />
          )}
          {view === "backups" && (
            <Backups
              backups={backups}
              busy={!!busy}
              onReload={() =>
                void run("Sicherungen laden", async () =>
                  setBackups(await api.listBackups()),
                )
              }
              onRestore={(id) => prepare({ kind: "restore", backup_id: id })}
            />
          )}
          {view === "codex" && (
            <Codex
              status={integration}
              result={registration}
              busy={!!busy}
              onReload={() =>
                void run("Codex-Status laden", async () =>
                  setIntegration(await api.getIntegrationStatus()),
                )
              }
              onRegister={() =>
                void run("Codex einrichten", async () => {
                  setRegistration(await api.registerCodex());
                  setIntegration(await api.getIntegrationStatus());
                })
              }
            />
          )}
          {plan && (
            <Preview
              key={plan.id}
              plan={plan}
              blocked={stale || !snapshot}
              busy={!!busy}
              onApply={apply}
              onDismiss={invalidatePlan}
            />
          )}
          {report && <Report report={report} />}
          <footer>
            BootPilot verwaltet vorhandene Bootoptionen. Ein
            Konfigurationsergebnis und ein erfolgreicher Boot-Test werden
            getrennt ausgewiesen.
          </footer>
        </main>
      </div>
    </div>
  );
}
