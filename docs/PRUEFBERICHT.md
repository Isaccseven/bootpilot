# Prüfstand der Windows-Vorabversion

Stand: 17.10.2026, Version 0.1.0. Dieser Bericht trennt automatisierte
Programmprüfungen von Installation, sichtbarer Bedienung und Hardware-Boot.
Die Vorabversion ist unsigniert. `boot_tested` bleibt ohne echten Neustarttest
`false`.

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

## Tatsächlich nachgewiesen und offene Abnahme

Die Paket-Guard-Suite wurde lokal zunächst mit fehlendem Implementierungsskript
fehlschlagend ausgeführt und anschließend erfolgreich: relative/root/nonempty
Ausgabeziele, leeres oder fehlendes erlaubtes Ziel, exakte ZIP-Struktur,
verbotenes Fixtureprogramm, leeres Binary, ungültiger PE-Header, falsche
Architektur und DLL statt EXE. Vorhandene Testdaten blieben erhalten.
Die Suite prüft außerdem den echten Paketeinstieg bei belegtem Ziel, die frühe
Ablehnung ungültiger Prüfverzeichnisse sowie ZIP-Traversal und doppelte Einträge
ohne Ausführung eines Fixtureprogramms; insgesamt 16 erfolgreiche Prüfungen.

Produktions-Sidecars für Windows x64, TypeScript-/Frontend-Build und generierte
Dependency-Hinweise wurden lokal erfolgreich gebaut. Der anschließende native
Releasebuild wurde von Windows Smart App Control beim Start eines unsignierten
`schemars`-Build-Skripts mit Betriebssystemfehler 4551 blockiert. Daher ist durch
diesen lokalen Paketlauf kein fertiger Installer oder portabler Laufzeittest
nachgewiesen. Die Sicherheitsrichtlinie wurde nicht verändert oder umgangen.
Actionlint und Syntax-/Fixtureprüfungen der echten Workflow-Schritte waren
erfolgreich; dies ist kein Ersatz für einen Windows-Runner-Lauf.

Die vollständige abschließende lokale Prüfung, ein aktueller vollständiger
GitHub-Windows-Lauf, die echte NSIS-Installation/Deinstallation, das Starten der
installierten Oberfläche, die tatsächliche Codex-Registrierung einschließlich
Erhalt anderer Server sowie die sichtbare Abnahme werden erst nach den letzten
Integrationsfixes gesondert protokolliert. Ein früher grüner Core-CI-Lauf ist
dafür kein Nachweis. Der Paketvertrag allein bestätigt weder UAC-Dialoge noch
die Laufzeitbedienung dieser konkreten Maschine.

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

Ein direkter MCP-Test bestätigt die installierte EXE, nicht die bereits laufende
Codex-Sitzung. Nach Registrierung kann ein vollständiger Codex-Neustart nötig
sein. Ein Status ohne neue Erfassung enthält nur einen vorhandenen Cache oder
explizit keinen Snapshot. Unvollständige Systemquellen werden nicht als
Abwesenheit eines Betriebssystems interpretiert.
