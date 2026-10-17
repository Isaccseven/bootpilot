# Architektur und Grenzen

`bootpilot.exe` ist die native Tauri-2-Oberfläche mit lokal eingebetteten
React-/TypeScript-Ressourcen. Die WebView lädt keine entfernte Anwendung. CSP,
Capabilities und eigene Commands beschränken den Zugriff; es gibt keine
allgemeine Shell- oder Dateisystemfreigabe.

`boot-core` enthält Datenmodell, Diagnose, GRUB-Erhaltung, Planvalidierung und
Transaktionen unabhängig von Tauri und MCP. `boot-windows` kapselt Windows-APIs,
Erfassung, UAC/IPC und geschützten Speicher. `boot-service` stellt dieselben
Anwendungsoperationen für die GUI, CLI und den mit dem offiziellen Rust-SDK
implementierten stdio-MCP-Server bereit. JSON ist versioniert und verwendet
`snake_case`; unbekannte Daten bleiben explizit unbekannt.

`boot-admin.exe` ist ein kurzlebiger erhöhter Helfer mit fest definierten
Aktionen. Er validiert Ziele, Argumente und Pläne selbst. GUI und MCP laufen
unprivilegiert; ein Planbezeichner allein ist keine Schreibberechtigung.
`boot-cli.exe` bietet unter anderem `status --json`, die ausdrücklich
anzufordernde Erfassung `refresh --json` und Codex-Integration. Der
Registrierungscode hat einen einzigen Rust-Schreibpfad mit Sicherung,
TOML-Bearbeitung und Dateisperre. PowerShell und GUI verwenden diesen Pfad.

Ein Plan bezieht sich auf eine konkrete Erfassung und gilt 15 Minuten. Der
Helfer vergleicht den erwarteten Zustand vor dem Schreiben nochmals, sichert
Originale dauerhaft, protokolliert Schritte und verifiziert Ergebnisse.
Unterbrochene Journale dürfen nicht als neue Pläne erneut ausgeführt werden.
Schreibadapter unterscheiden sicher ungeschriebene Schritte von möglicherweise
bereits geschriebenen Schritten. Die dauerhaften Belege halten diese Grenze
für gezielte Wiederherstellung fest. Ein bloßer Schreibvorsatz im Journal
berechtigt nach Unterbrechung auch bei passenden Ergebnisbytes nicht zum
automatischen Überschreiben; dieser Fall benötigt manuelle Untersuchung.
Dateischritte verweigern konkurrierendes Schreiben/Löschen über denselben
offenen Handle; sie sind nicht gegen Prozessabbruch oder Stromausfall atomar.
Firmware-Reihenfolge erhält einen erwarteten Zustandsvergleich und eine
nachgelagerte Prüfung, jedoch keinen atomaren Firmwarevergleich. Die manuelle
Wiederherstellungsgrenze steht in [BENUTZUNG.md](BENUTZUNG.md).

Schreibziele sind ausschließlich erkannte unterstützte GRUB-Dateien und die
Firmware-Reihenfolge. Keine frei übergebene Pfadzeichenkette wird zum erhöhten
Schreibziel. Ein vollständiger BCD-Import gehört nicht zur automatischen
Rücknahme. Microsoft-Loaderpfad, Partitionierung, Formatierung, Firmware-Updates
und automatische Secure-Boot-/BitLocker-Änderungen liegen außerhalb des Umfangs.

Die drei Produktions-Sidecars werden getrennt gebaut und Tauri mit dem
erforderlichen Target-Suffix bereitgestellt. NSIS entfernt dieses Suffix im
installierten Programmverzeichnis. Der portable Allowlist-Build kopiert genau
die vier Produktionsprogramme und benannte Dokumente/Skripte. Test-Fixtures,
Entwicklungsprobes, Diagnosen, private Konfigurationen und Toolchains gelangen
nicht in das Paket. Der Installer benutzt den offiziellen Tauri-NSIS-Ablauf für
Program Files und WebView2; es gibt keine eigenen Boot-/Registrierungs-Hooks.

REA diente ausschließlich zur Untersuchung der vorhandenen Integration und
Bootloader-Grenzen. Die datierten Untersuchungshinweise und die
[Entwurfsspezifikation](superpowers/specs/2026-10-17-bootpilot-design.md) dokumentieren
die daraus abgeleiteten Grenzen. Private REA-Rohdaten und reale Maschineninventare
sind kein Bestandteil der öffentlichen Pakete. Es wurde kein fremder
Grub2Win-Quelltext, keine Oberfläche und kein Bootloader übernommen.
