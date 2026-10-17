import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { changeLocale, formatDate, formatNumber } from "./i18n";
import { localizeBackendText } from "./localizeBackend";
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
} from "./types";
import "./styles.css";

type View = "overview" | "menu" | "backups" | "codex";
const views: { id: View; icon: string }[] = [
  { id: "overview", icon: "◫" },
  { id: "menu", icon: "☷" },
  { id: "backups", icon: "↶" },
  { id: "codex", icon: "⌘" },
];
const sources: Source[] = [
  "firmware",
  "bcd",
  "boot_current",
  "secure_boot",
  "bitlocker",
  "grub",
  "linux",
  "volumes",
];

function ErrorNotice({ error }: { error: BootError }) {
  const { t } = useTranslation();
  return (
    <div className="notice danger" role="alert">
      <strong>{localizeBackendText(error.message)}</strong>
      {error.details != null && (
        <details>
          <summary>{t("error.details")}</summary>
          <pre>
            {typeof error.details === "string"
              ? localizeBackendText(error.details)
              : JSON.stringify(error.details, null, 2)}
          </pre>
        </details>
      )}
      <small>{t("error.code", { code: error.code })}</small>
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
  const { t } = useTranslation();
  return (
    <section
      className={`panel result ${report.outcome === "partial" ? "warning-border" : ""}`}
      aria-label={t("report.label")}
    >
      <div className="section-heading">
        <div>
          <p className="eyebrow">{t("report.eyebrow")}</p>
          <h2>{t(`outcomes.${report.outcome}`)}</h2>
        </div>
        <span className="badge">
          {report.boot_tested ? t("report.bootTested") : t("report.bootUntested")}
        </span>
      </div>
      <p>
        {report.outcome === "applied"
          ? t("report.applied")
          : report.outcome === "partial"
            ? t("report.partial")
            : t("report.rolledBack")}
      </p>
      <dl className="metadata">
        <div>
          <dt>{t("common.plan")}</dt>
          <dd>{report.plan_id}</dd>
        </div>
        <div>
          <dt>{t("common.backup")}</dt>
          <dd>{report.backup_id ?? t("report.noBackup")}</dd>
        </div>
      </dl>
      {report.verified_steps.length > 0 && (
        <ul className="plain-list">
          {report.verified_steps.map((step) => (
            <li key={`${step.sequence}-${step.phase}`}>
              {t(`targets.${step.target}`)} · {t(`phases.${step.phase}`)}
              {step.detail && ` · ${localizeBackendText(step.detail)}`}
            </li>
          ))}
        </ul>
      )}
      {report.remaining_deviations.length > 0 && (
        <div className="notice warning">
          <h3>{t("report.deviations")}</h3>
          <ul>
            {report.remaining_deviations.map((deviation, i) => (
              <li key={i}>
                <strong>{t(`targets.${deviation.target}`)}:</strong>{" "}
                {localizeBackendText(deviation.description)}
              </li>
            ))}
          </ul>
        </div>
      )}
      {!report.boot_tested && (
        <p className="muted">{t("report.bootTestExplanation")}</p>
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
  const { t } = useTranslation();
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
    <section className="panel preview" aria-label={t("preview.label")}>
      <div className="section-heading">
        <div>
          <p className="eyebrow">{t("preview.eyebrow")}</p>
          <h2>{t("preview.label")}</h2>
        </div>
        <span className="badge blue">{t(`requests.${plan.request.kind}`)}</span>
      </div>
      <p>{t("preview.explanation")}</p>
      <dl className="metadata">
        <div>
          <dt>{t("preview.planId")}</dt>
          <dd>{plan.id}</dd>
        </div>
        <div>
          <dt>{t("preview.expires")}</dt>
          <dd>{formatDate(plan.expires_at)}</dd>
        </div>
        <div>
          <dt>{t("preview.snapshot")}</dt>
          <dd>{plan.snapshot_id}</dd>
        </div>
      </dl>
      {plan.diffs.map((diff, index) => (
        <article className="diff" key={`${diff.target}-${index}`}>
          <h3>{t(`targets.${diff.target}`)}</h3>
          <p>{localizeBackendText(diff.description)}</p>
          <div className="diff-grid">
            <div>
              <h4>{t("preview.before")}</h4>
              <pre>{diff.before || t("preview.empty")}</pre>
            </div>
            <div>
              <h4>{t("preview.after")}</h4>
              <pre>{diff.after || t("preview.empty")}</pre>
            </div>
          </div>
        </article>
      ))}
      {plan.preconditions.length > 0 && (
        <details className="checksums">
          <summary>
            {t("preview.preconditions", {
              count: plan.preconditions.length,
              total: formatNumber(plan.preconditions.length),
            })}
          </summary>
          {plan.preconditions.map((condition) => (
            <p key={condition.target}>
              {t(`targets.${condition.target}`)}
              <code>{condition.expected_checksum}</code>
            </p>
          ))}
        </details>
      )}
      {expired && (
        <p className="notice warning" role="status">{t("preview.expired")}</p>
      )}
      {blocked && (
        <p className="notice warning">{t("preview.noCurrentSnapshot")}</p>
      )}
      {plan.state !== "ready" && (
        <p className="notice warning">{t("preview.started")}</p>
      )}
      {plan.diffs.length === 0 && (
        <p className="notice warning">{t("preview.noChanges")}</p>
      )}
      <label className="confirmation">
        <input
          type="checkbox"
          checked={checked}
          disabled={unavailable || busy}
          onChange={(event) => setChecked(event.target.checked)}
        />
        {t("preview.confirmation")}
      </label>
      <div className="actions">
        <button
          className="primary"
          disabled={!checked || unavailable || busy}
          onClick={onApply}
        >
          {busy ? t("preview.applying") : t("preview.apply")}
        </button>
        <button disabled={busy} onClick={onDismiss}>{t("preview.dismiss")}</button>
        <span className="muted">{t("preview.adminPrompt")}</span>
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
  const { t } = useTranslation();
  const current = snapshot.boot_current;
  return (
    <>
      <div className="summary-grid">
        <section
          className="panel summary-card"
          role="group"
          aria-label={t("common.secureBoot")}
        >
          <span className="small-icon">◇</span>
          <h2>{t("common.secureBoot")}</h2>
          <strong>
            {snapshot.secure_boot === null
              ? t("common.unknown")
              : snapshot.secure_boot
                ? t("overview.enabled")
                : t("overview.disabled")}
          </strong>
          <p>
            {t("overview.source", {
              state: t(`states.${snapshot.sources.secure_boot}`),
            })}
          </p>
        </section>
        <section
          className="panel summary-card"
          role="group"
          aria-label={t("common.bitlocker")}
        >
          <span className="small-icon">▣</span>
          <h2>{t("common.bitlocker")}</h2>
          <strong>
            {snapshot.bitlocker === null
              ? t("common.unknown")
              : snapshot.bitlocker
                ? t("overview.protectionEnabled")
                : t("overview.protectionDisabled")}
          </strong>
          <p>
            {t("overview.systemVolume", {
              state: t(`states.${snapshot.sources.bitlocker}`),
            })}
          </p>
        </section>
        <section className="panel summary-card">
          <span className="small-icon">↗</span>
          <h2>{t("overview.currentBoot")}</h2>
          <strong>
            {current
              ? `Boot${current.uefi_number.toString(16).toUpperCase().padStart(4, "0")}`
              : t("common.unknown")}
          </strong>
          <p>
            {current?.description ?? t("overview.noBootMapping")}
          </p>
        </section>
      </div>
      <div className="overview-grid">
        <section className="panel">
          <div className="section-heading">
            <div>
              <p className="eyebrow">{t("common.uefi")}</p>
              <h2>{t("overview.bootOrder")}</h2>
            </div>
            <span className="badge">
              {t(`states.${snapshot.sources.firmware}`)}
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
                    <span className="entry-number">{formatNumber(index + 1)}</span>
                    <div>
                      <strong>
                        {entry?.title ?? t("overview.unknownFirmware")}
                      </strong>
                      <small>{entry?.loader_path ?? id}</small>
                    </div>
                  </li>
                );
              })}
            </ol>
          ) : (
            <Empty title={t("overview.noOrder")}>{t("overview.emptyNotEvidence")}</Empty>
          )}
          {snapshot.boot_next !== null && (
            <p className="notice info">
              {t("overview.nextBoot", {
                boot: `Boot${snapshot.boot_next.toString(16).toUpperCase().padStart(4, "0")}`,
              })}
            </p>
          )}
        </section>
        <section className="panel">
          <div className="section-heading">
            <div>
              <p className="eyebrow">{t("common.grub")}</p>
              <h2>{t("overview.menuEntries")}</h2>
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
                    <small>{t(`kinds.${entry.kind}`)}</small>
                  </div>
                  {snapshot.grub?.default_entry_id === entry.id && (
                    <span className="badge">{t("overview.default")}</span>
                  )}
                </li>
              ))}
            </ul>
          ) : (
            <Empty title={t("overview.noGrubMenu")}>{t("overview.completeScanNeeded")}</Empty>
          )}
          {!snapshot.linux_candidates.some(
            (candidate) => candidate.validated,
          ) && (
            <p className="muted">{t("overview.noValidatedLinux")}</p>
          )}
        </section>
      </div>
      <section className="panel">
        <div className="section-heading">
          <div>
            <p className="eyebrow">{t("overview.quality")}</p>
            <h2>{t("overview.scanChecks")}</h2>
          </div>
          <button onClick={onDiagnose} disabled={busy}>{t("overview.diagnose")}</button>
        </div>
        <div className="source-grid">
          {sources.map((source) => (
            <div key={source}>
              <span>{t(`sources.${source}`)}</span>
              <span
                className={`state ${snapshot.sources[source] === "complete" ? "complete" : ""}`}
              >
                {t(`states.${snapshot.sources[source]}`)}
              </span>
            </div>
          ))}
        </div>
        {snapshot.source_errors.map((item, i) => (
          <div className="source-error" key={i}>
            <strong>{t(`sources.${item.source}`)}</strong>
            <ErrorNotice error={item.error} />
          </div>
        ))}
        {findings !== null &&
          (findings.length ? (
            <ul className="findings">
              {findings.map((finding, i) => (
                <li key={`${finding.code}-${i}`}>
                  <span className="badge">
                    {finding.certainty === "evidence" ? t("overview.evidence") : t("overview.hypothesis")}
                  </span>
                  <strong>{localizeBackendText(finding.statement)}</strong>
                  {finding.open_check && (
                    <p>
                      {t("overview.openCheck", {
                        check: localizeBackendText(finding.open_check),
                      })}
                    </p>
                  )}
                  {finding.evidence.length > 0 && (
                    <small>
                      {finding.evidence
                        .map(
                          (evidence) =>
                            `${t(`sources.${evidence.source}`)}: ${evidence.reference}`,
                        )
                        .join(" · ")}
                    </small>
                  )}
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted">{t("overview.noFindings")}</p>
          ))}
      </section>
      {(snapshot.loader_info ||
        snapshot.loader_entry_selected ||
        snapshot.volumes.length > 0) && (
        <details className="panel technical">
          <summary>{t("overview.systemData")}</summary>
          {snapshot.loader_info && (
            <p>{t("overview.loader", { loader: snapshot.loader_info })}</p>
          )}
          {snapshot.loader_entry_selected && (
            <p>
              {t("overview.selectedLoader", {
                entry: snapshot.loader_entry_selected,
              })}
            </p>
          )}
          {snapshot.volumes.map((volume, i) => (
            <p key={volume.volume_id ?? i}>
              {volume.paths.join(", ") ||
                volume.volume_id ||
                t("overview.unmappedVolume")}{" "}
              · {volume.filesystem ?? t("overview.unknownFilesystem")}
              {volume.size_bytes !== null &&
                ` · ${formatNumber(volume.size_bytes / 1024 ** 3)} GiB`}
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
  const { t } = useTranslation();
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
            <p className="eyebrow">{t("menu.eyebrow")}</p>
            <h2>{t("menu.defaultTimeout")}</h2>
          </div>
          <span className="badge">
            {grub?.write_supported
              ? t("menu.supported")
              : t("menu.readOnly")}
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
                {t("menu.defaultEntry")}
                <select
                  value={defaultId}
                  onChange={(event) => {
                    setDefaultId(event.target.value);
                    invalidate();
                  }}
                  disabled={!canMenu}
                >
                  <option value="" disabled>{t("menu.selectEntry")}</option>
                  {grub.entries.map((entry) => (
                    <option key={entry.id} value={entry.id}>
                      {entry.title} · {t(`kinds.${entry.kind}`)}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                {t("menu.timeout")}
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
                {grub.write_block_reason
                  ? localizeBackendText(grub.write_block_reason)
                  : t("menu.unsupported")}
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
              {t("menu.preview")}
            </button>
          </form>
        ) : (
          <Empty title={t("menu.noGrub")}>{t("menu.refreshHelp")}</Empty>
        )}
      </section>
      <section className="panel">
        <div className="section-heading">
          <div>
            <p className="eyebrow">{t("menu.firmware")}</p>
            <h2>{t("menu.firmwareOrder")}</h2>
          </div>
        </div>
        <p className="muted">{t("menu.orderExplanation")}</p>
        {order.length ? (
          <>
            <ol className="entry-list reorder">
              {order.map((id, index) => {
                const name =
                  snapshot.firmware_entries.find((entry) => entry.id === id)
                    ?.title ?? id;
                return (
                  <li key={id}>
                    <span className="entry-number">{formatNumber(index + 1)}</span>
                    <strong>{name}</strong>
                    <div className="move-buttons">
                      <button
                        aria-label={t("menu.moveUp", { name })}
                        disabled={!canOrder || index === 0}
                        onClick={() => move(index, -1)}
                      >
                        ↑
                      </button>
                      <button
                        aria-label={t("menu.moveDown", { name })}
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
              {t("menu.previewOrder")}
            </button>
          </>
        ) : (
          <Empty title={t("menu.orderUnavailable")} />
        )}
        {!canOrder && !busy && (
          <p className="muted">{t("menu.completeFirmwareNeeded")}</p>
        )}
      </section>
      <section className="panel">
        <div className="section-heading">
          <div>
            <p className="eyebrow">{t("menu.validatedInstallation")}</p>
            <h2>{t("menu.addCachyos")}</h2>
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
                {t("menu.detectedInstallation")}
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
                {t("menu.title")}
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
                  {t("menu.checkedKernel")} <code>{candidate.kernel_path}</code> ·{" "}
                  {t("menu.filesystem")} <code>{candidate.filesystem_uuid}</code>
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
              {t("menu.previewCachyos")}
            </button>
          </form>
        ) : (
          <Empty title={t("menu.noInstallation")}>{t("menu.noInstallerUse")}</Empty>
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
  const { t } = useTranslation();
  return (
    <section className="panel">
      <div className="section-heading">
        <div>
          <p className="eyebrow">{t("backups.eyebrow")}</p>
          <h2>{t("backups.title")}</h2>
        </div>
        <button onClick={onReload} disabled={busy}>{t("backups.reload")}</button>
      </div>
      <p className="muted">{t("backups.explanation")}</p>
      {backups === null ? (
        <p role="status">
          {busy
            ? t("backups.loading")
            : t("backups.notLoaded")}
        </p>
      ) : backups.length === 0 ? (
        <Empty title={t("backups.emptyTitle")}>{t("backups.emptyHelp")}</Empty>
      ) : (
        <div className="backup-list">
          {backups.map((backup) => (
            <article key={backup.id} className="backup">
              <div className="section-heading">
                <div>
                  <h3>{t(`requests.${backup.request.kind}`)}</h3>
                  <p>{formatDate(backup.created_at)}</p>
                </div>
                <span
                  className={`badge ${backup.outcome === null ? "amber" : ""}`}
                >
                  {backup.outcome
                    ? t(`outcomes.${backup.outcome.outcome}`)
                    : t("backups.incomplete")}
                </span>
              </div>
              <dl className="metadata">
                <div>
                  <dt>{t("backups.id")}</dt>
                  <dd>{backup.id}</dd>
                </div>
                <div>
                  <dt>{t("common.plan")}</dt>
                  <dd>{backup.plan_id}</dd>
                </div>
              </dl>
              {backup.outcome === null && (
                <p className="notice warning">{t("backups.incompleteHelp")}</p>
              )}
              <details>
                <summary>{t("backups.targetsJournal")}</summary>
                {backup.targets.map((target) => (
                  <div className="backup-target" key={target.target}>
                    <strong>{t(`targets.${target.target}`)}</strong>
                    <p>
                      {t("backups.original")} <code>{target.original_checksum}</code>
                    </p>
                    <p>
                      {t("backups.applied")} <code>{target.applied_checksum}</code>
                    </p>
                  </div>
                ))}
                {backup.journal.length ? (
                  <ol>
                    {backup.journal.map((step) => (
                      <li key={`${step.sequence}-${step.phase}`}>
                        {formatDate(step.at)} · {t(`targets.${step.target}`)} ·{" "}
                        {t(`phases.${step.phase}`)}
                        {step.detail && ` · ${localizeBackendText(step.detail)}`}
                      </li>
                    ))}
                  </ol>
                ) : (
                  <p className="muted">{t("backups.noSteps")}</p>
                )}
              </details>
              {backup.outcome?.remaining_deviations.map((item, i) => (
                <p key={i} className="notice warning">
                  {t(`targets.${item.target}`)}: {localizeBackendText(item.description)}
                </p>
              ))}
              <button disabled={busy} onClick={() => onRestore(backup.id)}>{t("backups.restore")}</button>
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
  const { t } = useTranslation();
  return (
    <>
      <section className="panel">
        <div className="section-heading">
          <div>
            <p className="eyebrow">{t("codex.eyebrow")}</p>
            <h2>{t("codex.title")}</h2>
          </div>
          <button disabled={busy} onClick={onReload}>{t("codex.check")}</button>
        </div>
        {status ? (
          <>
            <div className="integration-status">
              <div>
                <span className="status-dot" />
                <strong>
                  {status.configured
                    ? t("codex.configured")
                    : t("codex.notConfigured")}
                </strong>
              </div>
              <span
                className={`badge ${status.connection_verified ? "blue" : ""}`}
              >
                {status.connection_verified
                  ? t("codex.connected")
                  : t("codex.notConnected")}
              </span>
            </div>
            <dl className="metadata">
              <div>
                <dt>{t("codex.server")}</dt>
                <dd>{status.mcp_path ?? t("codex.noPath")}</dd>
              </div>
            </dl>
            <p className="muted">{localizeBackendText(status.details)}</p>
          </>
        ) : (
          <p role="status">
            {busy
              ? t("codex.loading")
              : t("codex.unavailable")}
          </p>
        )}
        <button
          className="primary"
          disabled={busy || status === null}
          onClick={onRegister}
        >
          {status?.configured
            ? t("codex.recheckSetup")
            : t("codex.setup")}
        </button>
        <p className="muted">{t("codex.setupHelp")}</p>
        {result && (
          <div className="notice info" role="status">
            <strong>{t("codex.saved")}</strong>
            <p>
              {t("codex.configuration")} <code>{result.config_path}</code>
            </p>
            <p>
              {t("codex.backup")} <code>{result.backup_path}</code>
            </p>
            {result.restart_required && (
              <p>{t("codex.restart")}</p>
            )}
          </div>
        )}
      </section>
      <section className="panel">
        <p className="eyebrow">{t("codex.examplesEyebrow")}</p>
        <h2>{t("codex.examplesTitle")}</h2>
        <ul className="examples">
          <li>{t("codex.exampleStatus")}</li>
          <li>{t("codex.exampleDefault")}</li>
          <li>{t("codex.exampleRestore")}</li>
        </ul>
        <p className="muted">{t("codex.exampleHelp")}</p>
      </section>
    </>
  );
}

export default function App() {
  const { t, i18n } = useTranslation();
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
      void run("busy.backups", async () =>
        setBackups(await api.listBackups()),
      );
    if (next === "codex" && integration === null && !busy)
      void run("busy.integration", async () =>
        setIntegration(await api.getIntegrationStatus()),
      );
  }
  function refresh() {
    if (busy) return;
    invalidatePlan();
    void run("busy.scan", async () => {
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
    void run("busy.prepare", async () => {
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
    void run("busy.apply", async () => {
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
            BootPilot<small>{t("app.tagline")}</small>
          </span>
        </div>
        <nav aria-label={t("app.navigation")}>
          {views.map((item) => (
            <button
              key={item.id}
              className={view === item.id ? "active" : ""}
              aria-current={view === item.id ? "page" : undefined}
              onClick={() => navigate(item.id)}
            >
              <span aria-hidden="true">{item.icon}</span>
              {t(`views.${item.id}.label`)}
            </button>
          ))}
        </nav>
        <div className="sidebar-note">
          <span className="status-dot" />
          <strong>{t("app.local")}</strong>
          <p>
            {t("app.reviewedPreview")}
            <br />
            {t("app.traceableChanges")}
          </p>
          <small>{t("app.version")}</small>
        </div>
      </aside>
      <div className="workspace">
        <header className="topbar">
          <span>{t("app.configuration")}</span>
          <div className="topbar-controls">
            <span className="local-label">
              <span className="status-dot" />
              {t("app.windowsLocal")}
            </span>
            <label className="language-control">
              <span>{t("language.label")}</span>
              <select
                value={i18n.resolvedLanguage === "en" ? "en" : "de"}
                onChange={(event) => changeLocale(event.target.value)}
              >
                <option value="de">{t("language.de")}</option>
                <option value="en">{t("language.en")}</option>
              </select>
            </label>
          </div>
        </header>
        <main id="main-content">
          <div className="page-heading">
            <div>
              <p className="eyebrow">
                {t(`views.${view}.label`).toLocaleUpperCase(i18n.resolvedLanguage)}
              </p>
              <h1>{t(`views.${view}.title`)}</h1>
              <p>{t(`views.${view}.subtitle`)}</p>
            </div>
            <button
              className="refresh"
              disabled={!!busy || loading}
              onClick={refresh}
            >
              <span aria-hidden="true">↻</span>{t("app.refresh")}
            </button>
          </div>
          <div className="snapshot-bar">
            <span
              className={`status-dot ${stale || incomplete ? "amber" : ""}`}
            />
            <span>
              {loading
                ? t("snapshot.loading")
                : snapshot
                  ? t(stale ? "snapshot.staleCaptured" : "snapshot.captured", {
                      date: formatDate(snapshot.captured_at),
                    })
                  : t("snapshot.none")}
            </span>
            {snapshot && (
              <span className="badge">
                {incomplete ? t("snapshot.partial") : t("snapshot.complete")}
              </span>
            )}
          </div>
          {busy && (
            <div className="working" role="status">
              <span className="spinner" />
              {t(busy)} …{" "}
              <span className="muted">{t("app.navigationAvailable")}</span>
            </div>
          )}
          {error && <ErrorNotice error={error} />}
          {stale && snapshot && (
            <div className="notice warning">{t("snapshot.staleHelp")}</div>
          )}
          {!loading &&
            !snapshot &&
            (view === "overview" || view === "menu") && (
              <section className="panel">
                <Empty title={t("snapshot.firstScan")}>{t("snapshot.firstScanHelp")}</Empty>
              </section>
            )}
          {snapshot && view === "overview" && (
            <Overview
              snapshot={snapshot}
              findings={findings}
              busy={!!busy}
              onDiagnose={() =>
                void run("busy.diagnose", async () =>
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
                void run("busy.backups", async () =>
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
                void run("busy.integration", async () =>
                  setIntegration(await api.getIntegrationStatus()),
                )
              }
              onRegister={() =>
                void run("busy.register", async () => {
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
          <footer>{t("app.footer")}</footer>
        </main>
      </div>
    </div>
  );
}
