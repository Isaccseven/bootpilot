# Prüfstand der Windows-Vorabversion

Stand: 17.10.2026, Version 0.1.1. Dieser Bericht trennt automatisierte
Programmprüfungen von Installation, sichtbarer Bedienung und Hardware-Boot.
Die Vorabversion ist unsigniert. `boot_tested` bleibt ohne echten Neustarttest
`false`.

## Ergänzung für 0.1.1: Sprachen

Die Oberfläche unterstützt Deutsch und Englisch mit lokal gespeicherter
Sprachwahl. 49 Frontend-Tests prüfen unter anderem den Sprachwechsel bei
laufenden Anfragen, den Erhalt von Entwürfen und Planbestätigungen, unveränderte
IPC-Aufrufe und Bootdaten, verweigerten Browser-Speicher und die vollständige
DE/EN-Katalogstruktur. TypeScript und der Produktions-Frontend-Build bestehen.
Eine unabhängige Prüfung der i18n-Änderung ist abgeschlossen. Die unten genannten
CI-Artefakte und Hardware-/Installationsgrenzen beziehen sich auf 0.1.0; aktuelle
Builds sind in GitHub Actions am jeweiligen Commit erkennbar.

## Automatisierter Prüfvertrag

Die Windows-CI führt mit den eingecheckten Lockfiles Rust-Formatierung,
Workspace-Clippy mit Warnungen als Fehler, sämtliche Workspace-Tests,
TypeScript-Prüfung, UI-Tests, Frontend-Build, Produktions-Sidecar-Build und den
neun Werkzeuge umfassenden MCP-Fixturetest aus. Das App-Paket wird nicht vom
Workspace ausgeschlossen. Sidecars werden vor Clippy und Tests bereitgestellt.
Fixturetests verwenden getrennte Programme und temporäre Daten, keine echten
Bootänderungen und keine UAC-Anforderung.

Die Paketprüfung validiert ein absolutes leeres/fehlendes Ausgabeziel,
Windows-x64-PE-Dateien und die genaue portable Dateiliste. Die erzeugte portable
Fassung startet CLI-Status und MCP `initialize`, `tools/list` und
`boot_get_status` aus einem leeren Arbeitsverzeichnis. Sie benötigt dafür keinen
Quellbaum, Node.js oder Rust. Installer und Administratorhelfer werden dabei
nicht gestartet. Ein Release prüft die Ausgabemenge und SHA-256-Werte nochmals,
bevor ein separater Job die Vorabversion publiziert.

## Tatsächlich nachgewiesen

Der vollständige [Windows-CI-Lauf 39046311499](https://github.com/Isaccseven/bootpilot/actions/runs/39046311499)
für Quellstand [dc174ac](https://github.com/Isaccseven/bootpilot/commit/dc174ac06bf8d535e727ff993ff96439a03b7150)
war erfolgreich. Nachgewiesen sind:

- Rust-Formatierung und Workspace-Clippy mit Warnungen als Fehler;
- 118 Rust- und 22 UI-Tests sowie TypeScript- und Frontend-Build;
- 16 Paket-Guard-Prüfungen gegen falsche Ausgabeziele, ungültige PE-Dateien,
  verbotene ZIP-Inhalte, Traversal und doppelte Einträge;
- der MCP-Fixturetest aller neun Werkzeuge, Produktions-Sidecars und nativer Build;
- der normale Produktions-CLI-/MCP-Test aus einem leeren Arbeitsverzeichnis;
- der optimierte Paketbau einschließlich NSIS-Installer und portablem ZIP.

Das CI-Artefakt `windows-preview` mit Kennung `12106177261` wurde heruntergeladen.
Sein vollständiger Artefaktdigest und beide SHA-256-Werte aus `SHA256SUMS.txt`
wurden lokal geprüft. Dateiliste und x64-PE-Header des portablen Pakets wurden
ebenfalls geprüft. Anschließend bestand das entpackte Paket lokal den
eigenständigen `verify-package.ps1`-Test: CLI-Status, MCP-Initialisierung,
Werkzeugliste mit neun Werkzeugen und lesender Status, ohne Quellbaumzugriff,
Node.js, Rust, neue Booterfassung oder erhöhte Rechte.

Auch die portable `bootpilot.exe` aus demselben CI-Artefakt wurde als normaler
Benutzer gestartet und sichtbar geprüft. Die native Übersicht zeigte echte,
bereits vorhandene Erfassungsdaten. Die Codex-Ansicht bestätigte den direkten
MCP-Test mit `initialize`, `tools/list` und `boot_get_status`. Vorhandene
Cachedaten bestätigen dabei keine neue Erfassung durch den aktuellen Helfer.

Ein früherer lokaler Paketbau war an Windows Smart App Control beim Start eines
unsignierten Build-Skripts gescheitert. Ein einzelner späterer lokaler Lauf des
überarbeiteten Quellstands wurde nach erfolgreicher CI-Paketerstellung beendet
und gilt als unterbrochen, weder als erfolgreich noch als fehlgeschlagen. Keine
Sicherheitsrichtlinie wurde verändert oder umgangen. Die oben genannte
Paketvalidierung bezieht sich ausdrücklich auf das heruntergeladene CI-Artefakt.

## Noch offene sichtbare Abnahme

Installation, Deinstallationseintrag und Programmverknüpfung, Start und sichtbare
Bedienung der installierten Oberfläche sowie tatsächliche Codex-Registrierung
einschließlich Erhalt anderer Server sind noch nicht bestätigt. Eine frische
Erfassung durch den erhöhten Helfer und eine darauf beruhende Vorschau wurden
mit diesem finalen Artefakt ebenfalls noch nicht abgenommen. Die Prüfung des
Deinstallationseintrags ist von einer tatsächlich ausgeführten Deinstallation
zu unterscheiden; eine Deinstallation wurde nicht getestet. Die erfolgreiche
portable Abnahme ersetzt diese offenen Schritte nicht.

Während der Entwicklung wurden keine echten Boot-Schreibtests oder Neustarts
als Testmaßnahme durchgeführt. Reale Bootdaten und vorherige/nachherige
Prüfsummen gehören in ein privates Abnahmeprotokoll und werden nicht in
öffentliche Pakete aufgenommen. Ein tatsächlicher Windows-/CachyOS-Start wurde
durch diese Entwicklung nicht nachgewiesen.

## Verbleibende technische Grenzen

Ein Abbruch während eines Dateischritts kann teilweise geschriebene Daten
hinterlassen. Die dauerhafte Sicherung und das Journal ermöglichen gezielte
Untersuchung; automatische Rücknahme lehnt nicht passende Fremd-/Teilzustände
ab. Firmwareoperationen sind nicht atomar vergleichbar. Siehe die konkrete
manuelle Wiederherstellungsanleitung in [BENUTZUNG.md](BENUTZUNG.md).
Ein dauerhafter Schreibvorsatz allein ist nach einer Unterbrechung kein
ausreichender eigener Schreibbeleg, selbst wenn aktuelle Bytes dem geplanten
Ergebnis entsprechen. Solche Fälle verlangen manuelle Untersuchung.

Ein direkter MCP-Test bestätigt die geprüfte EXE, nicht die bereits laufende
Codex-Sitzung. Nach Registrierung kann ein vollständiger Codex-Neustart nötig
sein. Ein Status ohne neue Erfassung enthält nur einen vorhandenen Cache oder
explizit keinen Snapshot. Unvollständige Systemquellen werden nicht als
Abwesenheit eines Betriebssystems interpretiert.
