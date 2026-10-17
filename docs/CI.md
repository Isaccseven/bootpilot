# Windows CI und Vorabversionen

`.github/workflows/ci.yml` prüft jeden Push und Pull Request auf einem frischen
`windows-2025`-Runner. Ein neuer Lauf desselben Branches beziehungsweise Pull
Requests beendet den vorherigen Lauf. Der Workflow hat ausschließlich
`contents: read`; Checkout speichert keine Git-Zugangsdaten.

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

## Frontend während des Aufbaus

Solange `package-lock.json` fehlt, überspringt CI ausdrücklich die sieben
Frontend-Schritte und schreibt den Grund in die Laufzusammenfassung. Ein grüner
Lauf in diesem Zustand bestätigt ausschließlich die vorhandenen Rust-Pakete;
er bestätigt keine fertige Tauri-App, Oberfläche, Paketierung oder MCP-Integration.
Zum Zeitpunkt der Workflow-Einführung fehlte das Frontend-Lockfile noch.

Sobald `package-lock.json` eingecheckt ist, laufen mit Node.js 24 automatisch:

```powershell
npm ci
npm run typecheck
npm run test:ui
npm run build
cargo build --locked --package boot-mcp
npm run test:mcp
```

Der Frontend-Build erfolgt vor den Rust-Workspace-Prüfungen, damit Tauri seine
lokalen Webressourcen beim Kompilieren vorfindet. Sobald Aufgabe 7 Tauri-Sidecars
über `externalBin` einbindet, muss deren Vorbereitung in **beiden** Workflows vor
`cargo clippy` und `cargo test` ergänzt werden: `build.rs` beziehungsweise die
Tauri-Makros können die Dateien bereits beim Kompilieren benötigen. Der konkrete
Vorbereitungsschritt wird mit dem Build-Skript aus Aufgabe 7 verbunden. Bis diese
Integration geprüft ist, bestätigt ein älterer grüner Core-Lauf keine erfolgreiche
Tauri-Kompilierung; das App-Paket darf bei den Workspace-Prüfungen nicht
ausgeschlossen werden.

Fehlende npm-Skripte oder das fehlende Paket `boot-mcp` lassen den Lauf dann
fehlschlagen; sie werden nicht stillschweigend übersprungen. `test:mcp` muss
temporäre Daten beziehungsweise Fixtures verwenden und darf weder UAC noch
Operationen an der echten Bootkonfiguration auslösen. Auch die Rust-Tests müssen
ohne erhöhte Rechte und ohne Änderungen an der Bootkonfiguration funktionieren.

## Release-Schnittstelle für die Paketierung

`.github/workflows/release.yml` startet bei einem gepushten `v*`-Tag oder manuell
mit einem **bereits vorhandenen** Tag, etwa `v0.1.0`. Er checkt genau diesen Tag
aus und akzeptiert eine numerische dreiteilige Version mit optionalem
Vorabversionssuffix. Die Versionen von `boot-core`, `boot-mcp`, `boot-cli` und
`boot-admin` müssen zum Tag passen.

Fehlende Lockfiles, Tauri-Konfiguration oder `scripts/package.ps1` brechen den
Release bereits vor dem Build mit einer konkreten Fehlermeldung ab. Die oben
genannten Rust-, Frontend- und MCP-Prüfungen sind beim Release verpflichtend.

Die von Aufgabe 7 bereitzustellende Build-Schnittstelle lautet:

```powershell
pwsh -NoProfile -File ./scripts/package.ps1 -OutputDirectory <absoluter-Ausgabepfad>
```

Der Aufrufer übergibt einen noch nicht vorhandenen Ausgabepfad. Das Skript muss
die Windows-x64-Release-Binaries und den NSIS-Installer bauen und bei jedem
unvollständigen oder fehlgeschlagenen Build einen Fehlerexitcode zurückgeben.
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

Der Workflow prüft die genaue Dateimenge, Dateigrößen, den Windows-EXE-Header des
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

Die erste tatsächliche GitHub-Ausführung erfolgt erst nach Review und Push.
Lokale Syntax- und Fixture-Prüfungen sind kein Nachweis eines erfolgreichen
GitHub-Runners oder eines vollständig gebauten Installers.
