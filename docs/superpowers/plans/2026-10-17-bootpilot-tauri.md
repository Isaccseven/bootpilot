# BootPilot Tauri Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eine startbare Windows-App zur Verwaltung der vorhandenen GRUB-/UEFI-Konfiguration bauen, die Codex über dieselben strukturierten Funktionen steuern kann.

**Architecture:** Tauri 2 mit React/TypeScript stellt die Oberfläche bereit. Ein unabhängiger Rust-Kern bedient die App, eine lokale CLI und einen separaten rmcp-Server; ein kurzlebiger UAC-Helfer kapselt geschützte Windows-Zugriffe. Änderungen durchlaufen einen geprüften Plan mit Vorschau, Sicherung, Zustandsvergleich und gezielter Rücknahme.

**Tech Stack:** Windows x64, Rust stable mindestens 1.90 / MSVC, Tauri 2.12.4, tauri-build 2.7.3, Tauri CLI 2.12.3, React 19, TypeScript, Vite 8, rmcp 3.5.1, windows 0.62.2, Tokio, serde, schemars, Vitest und React Testing Library. Cargo.lock und package-lock.json werden mitgeliefert. Kein zusätzlicher KI-API-Schlüssel.

**Spec:** `docs/superpowers/specs/2026-10-17-bootpilot-design.md`; lesbare Kopie: `outputs/BootPilot-Entwurf.md` im übergeordneten Arbeitsverzeichnis. Die ausdrückliche Tauri-Auswahl ersetzt den früheren .NET-/WPF-Vorschlag.

## Global Constraints

- GUI und MCP laufen mit normalen Benutzerrechten.
- Die WebView lädt ausschließlich mitgelieferte lokale Inhalte.
- Die Oberfläche erhält keine allgemeine Shell- oder Dateisystemfreigabe; alle Bootfunktionen durchlaufen die Validierung des gemeinsamen Rust-Kerns.
- Der Helfer akzeptiert ausschließlich definierte Aktionen und geprüfte Argumente, keine freien Shell-Befehle oder Skripte.
- Keine Partitionierung, Formatierung, Löschung von Betriebssystemen, Firmware-Updates oder automatische Änderung von Secure Boot beziehungsweise BitLocker.
- Der Pfad des Microsoft-Windows-Bootmanagers wird nicht ersetzt.
- Die App verteilt in dieser Version keine fremden Bootloader-Binärdateien und übernimmt keine Grub2Win-Oberfläche oder Grafiken.
- Schreiben ist auf die bestätigte Grub2Win-Benutzersektion und unterstützte Menüeinstellungen begrenzt.
- Ein kompletter BCD-Import dient nicht als pauschale Rücknahme. Neustarts sind keine automatische Folge einer Änderung.
- Kein echter Boot-Schreibtest und kein PC-Neustart nur zur Entwicklung. Ein späterer Boot-Test wird separat dokumentiert.
- Alle Ansichten verwenden echte Systemdaten. Ein separat gekennzeichneter Demonstrationsmodus darf für Tests und Vorschauen verwendet werden.
- Bestehende Codex-MCP-Server, insbesondere REA, bleiben bei der Einrichtung erhalten.

## Review Focus

1. UAC-Abbruch oder fehlende Rechte: unvollständiger Status mit konkretem Grund, keine fälschliche Meldung „Linux fehlt“; Tests in Aufgabe 2.
2. Änderungen durch Grub2Win zwischen Vorschau und Anwenden: Plan ablehnen, keine fremden Änderungen überschreiben; Tests in Aufgaben 3 und 4.
3. Fremde GRUB-Syntax, CRLF/BOM, mehrfach definierte Defaults/Timeouts: Unbekanntes bytegenau erhalten oder Schreiben ablehnen; Tests in Aufgabe 3.
4. Fehler nach dem ersten Schreibschritt oder Prozessabbruch: dauerhaftes Journal und gezielte Rücknahme, kein pauschales BCD-Import; Tests in Aufgabe 4.
5. Manipulierte Pläne, Pipe-Clients oder Pfade: Helper validiert selbst, keine beliebige erhöhte Dateischreibfunktion; Tests in Aufgaben 2 und 4.

---

## Aufbau und gemeinsame Verträge

Repository: `work/bootpilot`. Auslieferung: `outputs/BootPilot`. Die Codeänderungen erfolgen auf `feature/tauri-bootpilot`; das neu angelegte Repository ist bereits vom übrigen Arbeitsverzeichnis isoliert.

| Pfad | Verantwortung |
| --- | --- |
| `Cargo.toml`, `rust-toolchain.toml`, `package.json` | Rust-Workspace, feste Toolchain und reproduzierbare Frontend-Kommandos |
| `crates/boot-core/src/{model,error,diagnose,plan,grub,transaction}.rs` | Gemeinsames Datenmodell, Diagnose, reine Planung und Transaktionen |
| `crates/boot-core/tests/{validation,grub,transactions}.rs` | Tests ohne echten Windows-Bootzugriff |
| `crates/boot-windows/src/{snapshot,bcd,firmware,volumes,broker,store}.rs` | Windows-Erfassung, native APIs, UAC/IPC und geschützter Speicher |
| `crates/boot-windows/tests/{inventory,broker,storage}.rs` | Rechte-, IPC- und Windows-Adaptertests |
| `crates/boot-admin/src/main.rs` | Fest begrenzter erhöhter Helfer ohne WebView |
| `crates/boot-service/src/{lib,client}.rs` | Gemeinsame Anwendungsschnittstelle für GUI, CLI und MCP |
| `crates/boot-mcp/src/{main,tools}.rs` | Offizieller rmcp-stdio-Server, keine eigene Bootlogik |
| `crates/boot-cli/src/main.rs` | Strukturierte Diagnose und kontrollierbarer Verbindungstest |
| `src-tauri/{Cargo.toml,build.rs,tauri.conf.json}` | Tauri-App und Windows-Paketierung |
| `src-tauri/src/{lib,commands}.rs`, `src-tauri/{capabilities,permissions}/` | Eng begrenzte Tauri-Kommandos und explizite Berechtigungen |
| `src/{App,api,types}.tsx`, `src/components/`, `src/styles.css` | Deutsche Oberfläche, Status, Vorschau, Sicherungen, Codex-Einrichtung |
| `tests/fixtures/`, `tests/mcp-smoke.mjs` | Anonymisierte Konfigurationsbeispiele, realer MCP-Protokolltest |
| `scripts/{build,package,register-codex,verify-package}.ps1` | Reproduzierbarer Build, Auslieferung und MCP-Registrierung |
| `docs/{BENUTZUNG,ARCHITEKTUR,PRUEFBERICHT}.md` | Bedienung, Sicherheitsgrenzen und tatsächlich ausgeführte Prüfungen |

`boot-core` kennt weder Tauri noch rmcp. JSON verwendet `snake_case`, RFC3339-UTC-Zeitstempel und explizite `null`-Werte für unbekannte Daten. `BootResult<T>` ist `Result<T, BootError>` mit maschinenlesbarem Fehlercode, deutscher Kurzmeldung und Details. Protokoll-/Dateiformate erhalten `schema_version: 1`.

- `Snapshot`: Erfassungs-ID, Zeitpunkt, Vollständigkeit pro Quelle, `firmware_entries`, `firmware_order`, optionaler `boot_current`, Secure-Boot-/BitLocker-Status, GRUB-Konfiguration, validierte Linux-Kandidaten und Quellenfehler.
- `PlanRequest`: ausschließlich `Order { entry_ids }`, `Menu { default_entry_id, timeout_seconds }`, `Cachyos { candidate_id, title }` oder `Restore { backup_id }`. Keine frei übergebenen Dateipfade oder Kernel-Befehlszeilen.
- `ChangePlan`: UUID, Ablaufzeit, Erfassungs-ID, geprüfte Anfrage, lesbare Diffs, Vorbedingungen und Prüfsummen. Gültigkeit 15 Minuten; UUID allein ist keine Autorisierung.
- `ApplyReport`: Plan-/Sicherungs-ID, Ergebnis `applied`, `rolled_back` oder `partial`, verifizierte Schritte und verbleibende Abweichungen. `boot_tested` bleibt ohne tatsächlichen Boot-Test `false`.
- `SystemAccess`: `read_snapshot(&mut self) -> BootResult<Snapshot>`, `read_target(&mut self, target: &Target) -> BootResult<TargetValue>`, `write_target(&mut self, target: &Target, expected: &TargetValue, replacement: &TargetValue) -> BootResult<()>`. `Target` erlaubt nur Firmware-Reihenfolge und die erkannten GRUB-Dateien; keine Pfadzeichenkette als Schreibziel. Der Write-Adapter prüft den erwarteten Wert unmittelbar vor dem Schreiben erneut; eine nicht atomar vergleichbare Firmware-Operation erhält zusätzlich eine nachgelagerte Prüfung.
- `PlanStore`: `save_plan(&mut self, plan: &ChangePlan) -> BootResult<()>`, `load_plan(&self, id: Uuid) -> BootResult<ChangePlan>`, `begin_backup(&mut self, plan: &ChangePlan, originals: &[(Target, TargetValue)]) -> BootResult<BackupRecord>`, `record_step(&mut self, id: Uuid, step: &JournalStep) -> BootResult<()>`, `finish_plan(&mut self, id: Uuid, outcome: &ApplyReport) -> BootResult<()>`, `list_backups(&self) -> BootResult<Vec<BackupRecord>>`, `load_backup(&self, id: Uuid) -> BootResult<BackupRecord>`. Wiederanlauf und Listen müssen auch unvollständige Journale sichtbar machen; ein begonnener Plan wird nicht erneut als frischer Plan ausgeführt.

### Task 1: Rust-Kern, Datenmodell und belegte Diagnose

**Dateien:** Workspace-Manifeste; `crates/boot-core/{Cargo.toml,src/lib.rs,src/model.rs,src/error.rs,src/diagnose.rs,tests/validation.rs}`; `tests/fixtures/`.

**Schnittstellen:** Definiert alle oben genannten Datentypen und Traits. Liefert `validate_request(request: &PlanRequest, snapshot: &Snapshot) -> BootResult<()>` und `diagnose(snapshot: &Snapshot) -> Vec<Finding>`. `Finding` enthält Schwere, Aussage, Evidenzreferenzen und gegebenenfalls eine offene Prüfung; Vermutung und belegter Befund sind getrennt.

- [ ] Isolierte Rust-Toolchain unter `work/tools` mit auf diesen Build beschränktem `RUSTUP_HOME`/`CARGO_HOME` installieren; vorhandenes Node 24, C++ Build Tools und WebView2 verwenden. Projektabhängigkeiten festhalten, globale Node-/Rust-Konfiguration nicht ersetzen.
- [ ] Failing Tests schreiben: `order_must_be_permutation` prüft doppelte/fehlende/fremde Firmware-IDs; `menu_requires_known_entry_and_timeout` prüft gültigen Eintrag und 1..=300 Sekunden; `unknown_security_state_stays_unknown` erwartet keine Umdeutung zu `false`; `diagnosis_distinguishes_evidence_from_hypothesis` erwartet für fehlende BootCurrent-Daten keine Behauptung, der Bootloader sei übersprungen worden.
- [ ] `cargo test -p boot-core --test validation` ausführen; fehlende Implementierung oder gezielte Assertions müssen scheitern.
- [ ] Typen und Validierung implementieren. IDs als UUID bzw. bekannte Menu-IDs normalisieren; keine Maschinen-GUIDs oder Bootpfade fest einbauen. Erfassungsfehler bleiben strukturierte Daten.
- [ ] Denselben Test erfolgreich ausführen, `cargo fmt --check` prüfen und als `feat: define shared boot model and diagnostics` committen.

### Task 2: Windows-Erfassung und begrenzter UAC-Helfer

**Dateien:** `crates/boot-windows/`, `crates/boot-admin/`; Tests `inventory.rs`, `broker.rs`, `storage.rs`.

**Schnittstellen:** Implementiert `WindowsSystem: SystemAccess` und `ProtectedStore: PlanStore`. Liefert `collect_snapshot() -> BootResult<Snapshot>` sowie `BrokerClient::request(&self, request: BrokerRequest) -> BootResult<BrokerResponse>`. `BrokerRequest` erlaubt `Refresh`, `Prepare(PlanRequest)`, `Apply { plan_id: Uuid }`, `ListBackups`; entsprechende Antworten sind `Snapshot`, `ChangePlan`, `ApplyReport`, `Vec<BackupRecord>`. Plan-/Apply-Handler werden in Aufgabe 4 vervollständigt; bis dahin expliziter Fehler `not_implemented`, kein simuliertes Erfolgsergebnis.

- [ ] Failing Tests für `access_denied_is_incomplete_inventory`, `uac_cancel_has_no_writes`, `unknown_message_is_rejected`, `wrong_pipe_peer_is_rejected`, `reparse_storage_path_is_rejected` schreiben. Windows-API-Aufrufe hinter injizierbaren Adaptern kapseln.
- [ ] `cargo test -p boot-windows` ausführen und erwartetes Scheitern festhalten.
- [ ] BCD strukturiert über den WMI-Provider erfassen; Volumes/Partitionen, Secure Boot und BitLocker nur abfragen. Native UEFI-Variablen ergänzen BootCurrent/BootNext und verfügbare Loader-Daten. Fehlende Rechte oder Variablen einzeln kennzeichnen. Falls BCDEdit tatsächlich nötig wird: nur feste Argumente, explizite Herkunft, deutsche und englische Test-Fixtures und Abbruch bei mehrdeutiger Ausgabe.
- [ ] Den Helper mit `ShellExecuteExW`, Verb `runas`, `SEE_MASK_NOCLOSEPROCESS` starten. Nur der Helper wird erhöht. IPC über zufällige lokale Named Pipe mit DACL für aufrufenden Benutzer, SYSTEM und Administratoren; Gegenstellen-PIDs und Nonce prüfen, Remote-Clients ablehnen, Nachrichtengröße begrenzen. UAC-Abbruch und Zeitüberschreitung eigenständig melden.
- [ ] Autoritative Pläne, Sicherungen und Journal unter einem geprüften, nicht umleitbaren ProgramData-Verzeichnis speichern. Normale Benutzer erhalten keinen Schreibzugriff auf fremde oder autoritative Plan-/Sicherungsdateien. Benutzercaches sind keine Quelle für erhöhte Schreibentscheidungen. Der Helper prüft seine Requests unabhängig vom Frontend.
- [ ] `cargo test -p boot-windows` erfolgreich ausführen. Ein echter lesender Refresh darf UAC erfordern; Status und Vollständigkeit mit vorhandenen Diagnosebefunden vergleichen, ohne Änderungen anzuwenden. Als `feat: collect Windows boot state through restricted helper` committen.

### Task 3: GRUB lesen und konkrete Änderungspläne erstellen

**Dateien:** `crates/boot-core/src/{grub,plan}.rs`, `crates/boot-core/tests/grub.rs`; GRUB-Fixtures unter `tests/fixtures/`.

**Schnittstellen:** `parse_grub(main: &[u8], user: &[u8]) -> BootResult<GrubDocument>`; `prepare_plan(snapshot: &Snapshot, request: PlanRequest, now: DateTime<Utc>) -> BootResult<ChangePlan>`. `GrubDocument` enthält Originalbytes, unterstützte Menüeinträge, Menüwerte und `write_supported` mit Begründung. `ChangePlan` verwendet ausschließlich `Target`/`TargetValue` aus Aufgabe 1.

- [ ] Failing Tests: `preserves_unmanaged_bytes_and_crlf`, `mismatched_usersection_blocks_write`, `dynamic_default_blocks_write`, `multiple_timeout_overrides_are_handled_explicitly`, `cachyos_uses_verified_managed_image_paths`, `kernel_tokens_cannot_inject_grub_code`, `order_plan_retains_all_entries`.
- [ ] `cargo test -p boot-core --test grub` ausführen; gezieltes Scheitern dokumentieren.
- [ ] Grub2Win-Marker und Benutzersektion strukturell erkennen. Hauptdatei und `userfiles/usersection.cfg` müssen denselben eingebetteten Abschnitt repräsentieren. Unbekannte Menüblöcke bleiben erhalten; dynamische oder mehrdeutige Einstellungen erlauben Anzeige, aber keine Änderung. Alle wirksamen Timeout-Overrides berücksichtigen, damit die Vorschau dem späteren Menü entspricht.
- [ ] Menüpläne verändern nur eindeutig erkannte Default-/Timeout-Zuweisungen und die beiden synchronen Benutzersektionen. Default-IDs werden auf die tatsächlich unterstützte GRUB-Referenz aufgelöst. CachyOS-Kandidaten stammen aus geprüftem Volume, vorhandenen Kernel-/Initramfs-Dateien und der bestehenden Konfiguration; Steuerzeichen, GRUB-Metazeichen, Traversal und nicht unterstützte Parameter ablehnen. Titel korrekt quoten; bestehende Installer/Fallbacks erhalten.
- [ ] Denselben Test erfolgreich ausführen. Erzeugte Fixture-Konfigurationen mit vorhandenem `grub-script-check` prüfen; dieser ergänzende Buildtest ersetzt nicht die eigene Validierung. Als `feat: preview validated GRUB and firmware changes` committen.

### Task 4: Anwenden, Sicherung und gezielte Wiederherstellung

**Dateien:** `crates/boot-core/src/transaction.rs`, `crates/boot-core/tests/transactions.rs`, geschützte Store-/Write-Adapter in `boot-windows`, Dispatch in `boot-admin`.

**Schnittstellen:** `apply_plan(system: &mut dyn SystemAccess, store: &mut dyn PlanStore, plan_id: Uuid, now: DateTime<Utc>) -> BootResult<ApplyReport>`; `prepare_restore(snapshot: &Snapshot, backup: &BackupRecord, now: DateTime<Utc>) -> BootResult<ChangePlan>`. Helper lädt Pläne aus seinem geschützten Store und prüft Anfrage, Grenzen und tatsächlichen Zustand erneut.

- [ ] Failing Tests gegen Temp-Dateien und Fake-Firmware: `expired_or_consumed_plan_cannot_apply`, `changed_snapshot_writes_nothing`, `change_between_preflight_and_write_is_rejected`, `backup_failure_writes_nothing`, `second_write_failure_rolls_back_first`, `rollback_refuses_intervening_change`, `incomplete_journal_is_visible_after_restart`, `restore_rejects_changed_post_state`, `gui_mcp_writes_are_serialized`.
- [ ] `cargo test -p boot-core --test transactions` ausführen und erwartetes Scheitern festhalten.
- [ ] Rechnerweiten Änderungs-Mutex verwenden; laufendes Grub2Win als Konflikt melden. Vorbedingungen nach Sperrübernahme erneut lesen. Sicherung samt Prüfsummen vor dem ersten Zielschreibzugriff dauerhaft speichern. Dateischritte mit überprüfter Dateiidentität und kontrolliertem Austausch ausführen, Firmware-Reihenfolge über feste BCD-Operation ändern; niemals Microsoft-Loaderpfad ändern. Nach jedem Schritt lesen und vergleichen.
- [ ] Journal vor und nach jedem Schreibschritt dauerhaft speichern. Bei Fehlern nur tatsächlich eigene Änderungen zurücknehmen, deren aktueller Wert noch dem geschriebenen Wert entspricht. Prozessabbruch erzeugt bei erneutem Start einen sichtbaren unvollständigen Vorgang; keine heimliche automatische Wiederherstellung. Sichere Rücknahme als neuen prüfbaren Plan anbieten. Erfolgreiche Pläne als verbraucht markieren.
- [ ] Tests erfolgreich ausführen und Fehlerbericht/Teilzustand prüfen. Keine Tests gegen echte Boot-Schreibziele ausführen. Als `feat: apply backed-up plans with guarded recovery` committen.

### Task 5: Gemeinsamer Dienst, CLI und Codex-MCP

**Dateien:** `crates/boot-service/`, `crates/boot-cli/`, `crates/boot-mcp/`, `tests/mcp-smoke.mjs`.

**Schnittstellen:** `BootService::{get_status, refresh, diagnose, prepare, apply, list_backups}`. Signaturen: `get_status(&self) -> BootResult<Option<Snapshot>>`, `refresh(&self) -> BootResult<Snapshot>`, `diagnose(&self) -> BootResult<Vec<Finding>>`, `prepare(&self, request: PlanRequest) -> BootResult<ChangePlan>`, `apply(&self, plan_id: Uuid) -> BootResult<ApplyReport>`, `list_backups(&self) -> BootResult<Vec<BackupRecord>>`. Längere Aufrufe dürfen asynchron gekapselt werden; Frontend und Server verwenden exakt diesen Dienst.

- [ ] Failing Protokolltests: `initialize_lists_expected_tools`, `missing_plan_id_is_invalid`, `status_has_freshness_and_completeness`, `stdout_contains_only_jsonrpc`, `errors_are_tool_errors_not_success_text`. Die neun Werkzeuge aus dem Entwurf müssen vorhanden sein, einschließlich `boot_plan_restore` als `prepare(Restore { backup_id })`.
- [ ] `npm run test:mcp` gegen die Testinstanz ausführen; erwartetes Scheitern ohne Server/Handler festhalten.
- [ ] rmcp mit `server`, `macros`, `transport-io` verwenden. JSON-Schemas aus Typen ableiten, strukturierte Resultate plus kurze deutsche Zusammenfassung liefern. Status-/Diagnose-/Sicherungslisten als lesend deklarieren; Refresh als möglicherweise interaktiv und Planer als lokale Plan-Erzeugung kennzeichnen; Apply als schreibend und potenziell destruktiv deklarieren. Keine erfundene Genehmigung durch ein Toolannotation-Flag.
- [ ] CLI `boot-cli status|refresh|diagnose|backups --json` ergänzen. Fehler auf stderr, definierte Exitcodes; MCP stdout ausschließlich JSON-RPC. Lokale Snapshot-Caches mit Erfassungszeit anzeigen; ohne Cache `null` und „Noch keine Erfassung“ statt Beispieldaten zurückgeben.
- [ ] `cargo test -p boot-service -p boot-mcp` und `npm run test:mcp` erfolgreich ausführen. Zusätzlich echte MCP-Initialisierung, Toolinventar und `boot_get_status`/lesende Erfassung starten. Als `feat: expose BootPilot through CLI and Codex MCP` committen.

### Task 6: Tauri-Oberfläche mit Vorschau und Ergebnissen

**Dateien:** `src-tauri/`, `src/`, `index.html`, TypeScript-/Vite-Konfiguration; Tests `src/{App,preview,backups}.test.tsx`.

**Schnittstellen:** Nur die Tauri-Kommandos `get_status`, `refresh`, `diagnose`, `prepare`, `apply`, `list_backups`, `get_integration_status` und `register_codex` bereitstellen. Daten entsprechen `BootService` ohne zweite Geschäftslogik. `src/api.tsx` kapselt invoke; `src/types.tsx` bildet die festgelegten JSON-Typen ab.

- [ ] Failing UI-Tests: `unknown_is_not_disabled_security`, `uac_cancel_keeps_last_snapshot_marked_stale`, `apply_requires_visible_plan`, `expired_plan_cannot_apply`, `partial_result_shows_backup_and_remaining_changes`, `installer_is_not_installed_linux`.
- [ ] `npm run test:ui` ausführen und gezielte Fehlschläge dokumentieren.
- [ ] Deutsche Oberfläche mit vier Bereichen bauen: Übersicht, Bootmenü, Sicherungen, Codex. Ruhige Gestaltung, Tastaturbedienung, sichtbare Lade-/Fehlerzustände, Aktualisieren-Aktion und Zeitstempel. Bootmenü zeigt vollständige Vorher-/Nachher-Vorschau, Ablaufzeit und betroffene Ziele; Anwenden erfordert die sichtbare konkrete Vorschau. Keine Erfolgsmeldung vor bestätigter Verifikation; „Konfiguration geprüft“ getrennt von „Boot getestet“ anzeigen.
- [ ] Tauri `AppManifest::commands` und eigene Permissions verwenden, Capability nur für Fenster `main`, kein Shell-/Filesystem-Plugin. Lokaler CSP, keine Remote-Skripte. UAC läuft im Rust-Backend, nicht im Browser; UI bleibt währenddessen bedienbar. `get_integration_status` zeigt eingerichteten Pfad und getrennt davon einen tatsächlich gemessenen Verbindungstest.
- [ ] `npm run test:ui`, `npm run typecheck`, `npm run build` und `cargo check -p bootpilot-app` erfolgreich ausführen. Windows-App starten und echte Daten sowie Vorschau-/Fehleransichten sichtbar kontrollieren. Als `feat: add Tauri boot management interface` committen.

### Task 7: Windows-Paket, Integration und Auslieferungsprüfung

**Dateien:** `scripts/`, `docs/`, Tauri-Bundlekonfiguration, eigener App-Icon-Satz; Ausgaben unter `outputs/BootPilot/`.

**Schnittstellen:** `register-codex.ps1 -McpPath <absolute-installed-exe>` erstellt bzw. aktualisiert ausschließlich `[mcp_servers.bootpilot]` nach validierter Pfadprüfung und vorheriger Sicherung. `verify-package.ps1 -PackageRoot <path>` prüft Paketdateien, Start und MCP-Handschlag ohne Bootmutation. GUI `register_codex` nutzt dieselbe eng begrenzte Registrierungsfunktion.

- [ ] Failing Tests für `registration_preserves_rea_and_other_servers`, `registration_is_idempotent`, `invalid_mcp_path_does_not_modify_config` und `installed_mcp_runs_without_source_tree` vorbereiten. TOML strukturiert und kommentarschonend bearbeiten; keine gesamte Config aus einem Minimaltemplate ersetzen.
- [ ] Registrierungstests gegen temporäre Config-Dateien ausführen; erwartetes Scheitern festhalten.
- [ ] Windows-x64-Release mit separaten `boot-mcp.exe`, `boot-cli.exe` und `boot-admin.exe` bauen. Tauri-Sidecars korrekt mit Target-Suffix paketieren. NSIS-Installer nach Program Files (`perMachine`) mit WebView2-Erkennung/Bootstrapper liefern; die installierte App selbst bleibt unprivilegiert. Paket nicht als signiert bezeichnen, sofern keine reale Codesignatur existiert.
- [ ] Testweise installieren und deinstallierbare App-Verknüpfung prüfen; vorhandene Bootloader bleiben unverändert. Codex-Konfiguration sichern, BootPilot eintragen und strukturell nachweisen, dass andere Server erhalten sind. Das installierte MCP-Binary direkt initialisieren und eine lesende Abfrage ausführen. Falls laufendes Codex neue Server erst nach Neustart lädt, diesen konkreten offenen Schritt benennen und direkte Protokollprüfung davon unterscheiden.
- [ ] `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `npm run test:ui`, `npm run typecheck`, `npm run test:mcp` sowie `verify-package.ps1` erfolgreich ausführen. Frische vollständige Prüfung einmal nach dem letzten Fix; weitere Wiederholungen nur bei Änderungen oder neuen Befunden.
- [ ] Unabhängiges Review von Helper-Grenzen, GRUB-Erhaltung, Transaktionen, MCP und Paket einholen; erhebliche Befunde beheben und betroffene Checks wiederholen. Vorher-/Nachher-Prüfsummen und Firmware-Reihenfolge dokumentieren, um unveränderte Bootkonfiguration während der Entwicklung nachzuweisen.
- [ ] Installer, Quellarchiv ohne Toolchains/Build-Caches, Bedienungsanleitung, REA-Evidenzverweis und Prüfbericht nach `outputs/BootPilot` liefern. Als `build: package and verify BootPilot for Windows` committen. Keine Behauptung eines erfolgreichen CachyOS-Starts ohne tatsächlichen Neustarttest.

## Selbstprüfung des Plans

Die vier UI-Bereiche sind Aufgabe 6 zugeordnet; Windows-Erfassung und UAC Aufgabe 2; Vorschau und GRUB-Erhaltung Aufgabe 3; Sicherung, Anwenden und Wiederherstellung Aufgabe 4; CLI/MCP Aufgabe 5; Paket, Registrierung und sichtbare Abnahme Aufgabe 7. Die fünf Review-Fälle haben jeweils benannte Tests. Die zentralen Typen und Dienstsignaturen werden einmal definiert und von beiden Bedienwegen konsumiert. REA-Erkenntnisse fließen als Integrationsgrenzen ein, ohne fremden GPL-Quelltext oder Bootloader-Binärdateien in das neue Programm zu übernehmen.

Die vorhandenen Werkzeuge Visual Studio Build Tools, Windows SDK und WebView2 wurden lesend gefunden; Rust muss eingerichtet werden. Das tatsächliche Bootverhalten nach dem bereits vorgenommenen Grub2Win-Wechsel bleibt eine getrennte offene Nutzerprüfung.

## Technische Referenzen

- [Tauri Windows-Voraussetzungen](https://v2.tauri.app/start/prerequisites/)
- [Tauri Capabilities und eigene Commands](https://v2.tauri.app/security/capabilities/)
- [Tauri Windows-Installer](https://v2.tauri.app/distribute/windows-installer/)
- [Tauri Hilfsprogramme](https://v2.tauri.app/develop/sidecar/)
- [Offizielles Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk)
- [Windows ShellExecuteEx / Prozesshandle](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-shellexecuteinfow)
