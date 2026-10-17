# Windows CI und Vorabversionen

`.github/workflows/ci.yml` prüft jeden Push und Pull Request auf einem frischen
`windows-2025`-Runner. Ein neuer Lauf desselben Branches beziehungsweise Pull
Requests beendet den vorherigen Lauf. Der Workflow hat ausschließlich
`contents: read`; Checkout speichert keine Git-Zugangsdaten.
Der normale CI-Lauf hat ein Zeitlimit von 60 Minuten. Nach allen App-Prüfungen
baut er bei Push und manueller Ausführung außerdem unsignierte NSIS-/Portable-
Prüfpakete und lädt exakt beide Dateien plus `SHA256SUMS.txt` als
`windows-preview`-Laufartefakt hoch. Bei Pull Requests entfallen ausschließlich
dieser Paketbau und Upload. Es wird dabei kein GitHub-Release veröffentlicht;
damit ist eine Installationsabnahme vor dem Anlegen eines Versionstags möglich.

Die Rust-Prüfungen verwenden die exakte Toolchain **1.99.1** aus
`rust-toolchain.toml`, einschließlich rustfmt und clippy:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Bei einer absichtlichen Toolchain-Änderung müssen sowohl `rust-toolchain.toml`
als auch die beiden Workflow-Prüfungen angepasst werden. `Cargo.lock` bleibt
verbindlich; CI aktualisiert keine Abhängigkeiten.

## Vollständige App-Prüfung

Das Frontend ist inzwischen verbindlich. Fehlende Lockfiles oder Skripte lassen
CI fehlschlagen; es gibt keinen Frontend-Skip mehr. Mit exakt Node.js 24.22.0
laufen:

```powershell
npm ci
npm run typecheck
npm run test:ui
npm run build
pwsh -NoProfile -File ./scripts/prepare-sidecars.ps1 -Profile Debug
pwsh -NoProfile -File ./scripts/test-package.ps1
npm run test:mcp
cargo build --locked --target x86_64-pc-windows-msvc --package bootpilot-app --bin bootpilot
pwsh -NoProfile -File ./scripts/verify-package.ps1 -PackageRoot "$PWD/target/x86_64-pc-windows-msvc/debug"
```

Frontend-Build und Produktions-Sidecars werden in **beiden** Workflows vor
`cargo clippy` und `cargo test` vorbereitet. `prepare-sidecars.ps1` baut
ausschließlich `boot-admin`, `boot-cli` und den normalen `boot-mcp` mit expliziten
Binärzielen für `x86_64-pc-windows-msvc`; es baut die App nicht rekursiv. Es
kopiert die von Tauri benötigten Target-Suffixdateien und erzeugt Lizenzhinweise.
Der Releaseworkflow verwendet dabei `-Profile Release`. Das native App-Paket ist
Teil aller Workspace-Prüfungen und wird danach als Produktionsprogramm gebaut.

`test:mcp` baut ein getrenntes `boot-mcp-fixture` mit ausdrücklich aktivierter
Testfunktion und prüft alle neun MCP-Werkzeuge gegen Testadapter. Das normale
Produktionsbinary wird dadurch nicht ersetzt. Es gibt keinen `--live`- oder
Refresh-Schritt in CI. Auch Rust-Tests laufen ohne erhöhte Rechte oder echte
Bootänderungen. Der anschließende Produktions-Pakettest startet nur CLI-Status
und MCP-Initialisierung/Werkzeugliste/Status aus einem leeren Temp-Verzeichnis.
Alle PowerShell-Schritte setzen die Textausgabe ausdrücklich auf UTF-8.

## Release-Schnittstelle für die Paketierung

`.github/workflows/release.yml` startet bei einem gepushten `v*`-Tag oder manuell
mit einem **bereits vorhandenen** Tag, etwa `v0.1.0`. Er checkt genau diesen Tag
aus und akzeptiert eine numerische dreiteilige Version mit optionalem
Vorabversionssuffix. Alle sieben Workspace-Paketversionen müssen zum Tag passen.
Das Paketskript vergleicht zusätzlich npm- und Tauri-Version.

Fehlende Lockfiles, Tauri-Konfiguration oder `scripts/package.ps1` brechen den
Release bereits vor dem Build mit einer konkreten Fehlermeldung ab. Die oben
genannten Rust-, Frontend- und MCP-Prüfungen sind beim Release verpflichtend.

Die Build-Schnittstelle lautet:

```powershell
pwsh -NoProfile -File ./scripts/package.ps1 -OutputDirectory <absoluter-Ausgabepfad>
```

Der Aufrufer übergibt einen absoluten noch nicht vorhandenen oder leeren
Ausgabepfad. `build.ps1` installiert npm-Abhängigkeiten mit `npm ci`, baut das
Frontend, bereitet Release-Sidecars vor und baut die App. `package.ps1` erstellt
anschließend NSIS und einen explizit zusammengestellten portablen ZIP-Inhalt.
Jeder unvollständige oder fehlgeschlagene Schritt ist ein Fehler.
Das Ausgabeziel enthält anschließend exakt diese zwei nicht leeren Dateien:

| Datei | Inhalt |
| --- | --- |
| `BootPilot-<version>-windows-x64-setup.exe` | Windows-x64-NSIS-Installer |
| `BootPilot-<version>-windows-x64.zip` | Portable Release-Dateien mit genau je einer `bootpilot.exe`, `boot-mcp.exe`, `boot-cli.exe`, `boot-admin.exe`, sowie erforderlichen Ressourcen, Lizenz und Dokumentation |

`<version>` enthält kein führendes `v`. Das Skript darf keine Diagnosen,
Sicherungen, privaten Konfigurationen, Schlüssel, Build-Caches oder sonstigen
Zwischendateien in dieses Ausgabeziel schreiben. Es darf den Installer oder
Helfer in CI nicht starten, keine UAC-Abfrage auslösen und keine echte
Bootkonfiguration lesen oder verändern.

Das Paketskript prüft x64-PE-Header aller vier Programme und die portable
Allowlist einschließlich Dokumenten, Skripten und generierten Lizenzhinweisen.
Es startet den portablen MCP direkt ohne Quellbaumabhängigkeit. Der Workflow
prüft die genaue Dateimenge, Dateigrößen, den Windows-EXE-Header des
Installers und die vier nicht leeren Binaries im ZIP. Er erzeugt danach
`SHA256SUMS.txt`. Diese Strukturprüfungen ersetzen weder eine Installationsprobe
noch einen echten GUI- oder Boot-Test. Ein leerer oder unvollständiger Build
kann keinen erfolgreichen Release erzeugen.

## Veröffentlichung und Berechtigungen

Der Windows-Build hat nur Leserechte. Er lädt ausschließlich die zwei geprüften
Pakete und die Prüfsummendatei als Laufartefakt hoch; fehlende Upload-Dateien sind
ein Fehler. Ein separater Ubuntu-Job erhält als einziger Job `contents: write`,
lädt das Artefakt herunter, prüft erneut die erwarteten drei Dateien und ihre
SHA-256-Prüfsummen und erstellt mit `gh release create --verify-tag --prerelease`
eine **GitHub-Vorabversion**. Bereits vorhandene Releases werden nicht
überschrieben. Release-Läufe desselben Tags laufen nacheinander.

Es werden keine API-Schlüssel oder Signierzertifikate benötigt oder mitgeliefert.
Eine Codesignatur wird durch diesen Workflow nicht zugesichert. Ein Release
bestätigt keinen erfolgreichen Betriebssystemstart; `boot_tested` benötigt
weiterhin einen separat durchgeführten echten Boot-Test.

Alle GitHub-Actions sind auf verifizierte unveränderliche Commit-SHAs ihrer
offiziellen `actions/*`-Repositorys gepinnt. Die Versionskommentare dienen nur
der Lesbarkeit. Bei Aktualisierungen Tag und Commit über die offizielle
Repository-Metadatenquelle prüfen und die Workflow-Syntax erneut validieren.

Der anfängliche Rust-CI-Lauf war erfolgreich, enthielt jedoch noch keine
vollständige App-Prüfung. Ein aktueller vollständiger GitHub-Lauf nach Integration
bleibt ein eigener Nachweis. Lokale Syntax- und Fixture-Prüfungen bestätigen
weder einen erfolgreichen Runner noch Installer-, GUI- oder Hardware-Abnahme.
Windows Smart App Control kann lokal auch neu kompilierte unsignierte
Build-Skripte und Programme blockieren. Das ist als konkreter Build-/Laufzeitfehler
zu dokumentieren; diese Skripte ändern keine Sicherheitsrichtlinien.
