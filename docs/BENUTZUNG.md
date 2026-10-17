# BootPilot verwenden

BootPilot 0.1.1 ist eine Windows-x64-Vorabversion. Installer und Programme sind
unsigniert. Ein bestandener Programmtest bestätigt keinen erfolgreichen
Windows- oder CachyOS-Start. Vor Bootänderungen müssen eigene Sicherungen und
gegebenenfalls der BitLocker-Wiederherstellungsschlüssel verfügbar sein.

## Installation und portabler Betrieb

Den NSIS-Installer `BootPilot-0.1.1-windows-x64-setup.exe` ausführen. Die
Installation für alle Benutzer verlangt UAC und verwendet Program Files. Falls
WebView2 fehlt, lädt der Installer den Microsoft-Bootstrapper; dafür wird eine
Internetverbindung benötigt. BootPilot selbst läuft anschließend ohne erhöhte
Rechte. Installation und Deinstallation ändern keine Bootkonfiguration und
installieren keine Bootloader. Es gibt einen regulären Deinstallationseintrag.

Alternativ das gesamte portable ZIP in ein dauerhaftes Verzeichnis entpacken und
`bootpilot.exe` starten. Alle vier EXE-Dateien müssen nebeneinander bleiben.
WebView2 muss bereits vorhanden sein. Nach Codex-Registrierung das Verzeichnis
nicht verschieben; andernfalls erneut registrieren. Das Entfernen einer
Programmdatei löscht weder die separat gespeicherten Sicherungen noch einen
bereits ausdrücklich vorgenommenen Bootänderungsschritt.

## Erfassung, Vorschau und Änderungen

Die Sprachauswahl oben rechts wechselt zwischen Deutsch und English und merkt
sich die Auswahl lokal. Deutsch bleibt die Standardsprache. Ein Sprachwechsel
erhält Formulareingaben und vorbereitete Pläne; er löst keine Systemabfrage aus.
Zeit- und Zahlenangaben folgen der gewählten Sprache. Eintragsnamen, Dateipfade,
Konfigurationsinhalte sowie unbekannte Originalmeldungen bleiben unverändert.
Die Übersetzungen sind im Programm enthalten und funktionieren ohne Internet.

1. In „Status“ ausdrücklich aktualisieren. Ein UAC-Dialog kann für geschützte
   Lesedaten erscheinen. Bei Abbruch bleiben die betroffenen Quellen unbekannt;
   unbekannt bedeutet nicht, dass Linux oder ein Bootloader fehlt.
2. Quellenfehler, Erfassungszeit und Diagnose prüfen. Die Oberfläche zeigt echte
   Daten; Tests verwenden getrennte, als solche erkennbare Adapter.
3. Für unterstützte Bootreihenfolge, Menüstandard, Zeitlimit oder einen erkannten
   CachyOS-Kandidaten eine Vorschau erzeugen. Freie Dateipfade und beliebige
   Kernelbefehle werden nicht als Schreibaufträge angenommen.
4. Diff und betroffene Ziele prüfen. Erst die ausdrückliche Bestätigung wendet
   einen aktuellen Plan an; geschützte Schritte fordern UAC an. Pläne laufen nach
   15 Minuten ab. Zwischenzeitliche Änderungen führen zur Ablehnung.
5. Ergebnis und verbleibende Abweichungen prüfen. Ein Neustart erfolgt niemals
   automatisch. Nach einem Änderungsversuch ist eine neue Erfassung erforderlich.

BootPilot partitioniert und formatiert nicht, löscht keine Betriebssysteme und
ändert Secure Boot oder BitLocker nicht automatisch. Der Microsoft-Loaderpfad
wird nicht ersetzt. Unterstützt werden vorhandene Grub2Win-Benutzersektionen und
geprüfte Menüeinstellungen; unbekannte Syntax muss erhalten bleiben oder wird
als nicht sicher schreibbar abgelehnt.

## Sicherungen und unterbrochene Vorgänge

Sicherungen und Journale werden im geschützten BootPilot-Speicher unter
`%ProgramData%` verwaltet. Unvollständige Vorgänge bleiben sichtbar. Eine
Wiederherstellung ist ein neuer geprüfter Plan mit Vorschau und UAC, kein
pauschaler BCD-Import. Fremde Änderungen werden nicht überschrieben.

Dateischritte verwenden exklusives Schreiben über denselben Handle mit
erneuter Zustandsprüfung, jedoch keinen absturzatomaren Dateiaustausch. Ein
Prozessabbruch oder Stromausfall kann teilweise geschriebene Bytes hinterlassen.
Wenn der aktuelle Inhalt weder Original noch erwartetes Änderungsergebnis ist,
lehnt die automatische Rücknahme beziehungsweise Wiederherstellung ihn ab.
Eine automatische Wiederherstellung braucht außerdem einen dauerhaft
protokollierten Schreib- oder Unsicherheitsbeleg des betreffenden Schritts.
Ein vor dem Abbruch gespeicherter Schreibvorsatz allein beweist keine eigene
Änderung: Auch passende Ergebnisbytes berechtigen dann nicht zur automatischen
Rücknahme. Ein solcher unklarer Fall erfordert manuelle Untersuchung.

In diesem Fall aktuelle Bytes, geschützte Sicherung und Journal erhalten. In
einer vertrauenswürdigen Administrator- oder Wiederherstellungsumgebung die
betroffenen Dateien und Prüfsummen untersuchen und ausschließlich die geprüften
betroffenen GRUB-Dateien aus der passenden Sicherung rekonstruieren. Keine
fremden Dateien überschreiben und keinen vollständigen BCD-Import als
Universalreparatur verwenden. Firmware-Reihenfolge wird unmittelbar vor dem
Schreiben erneut geprüft und danach verifiziert; die Firmwareoperation selbst
ist nicht atomar vergleichbar.

## Codex verbinden

In „Codex“ den installierten MCP-Pfad prüfen und die Registrierung ausdrücklich
ausführen. Alternativ mit PowerShell 7:

```powershell
pwsh -NoProfile -File .\scripts\register-codex.ps1 -McpPath 'C:\Program Files\BootPilot\boot-mcp.exe'
```

Der tatsächliche Installationspfad kann abweichen. Die Registrierung sichert die
vorherige `config.toml` und bearbeitet ausschließlich den Server `bootpilot`.
Andere Server, insbesondere REA, bleiben erhalten. Bei Abbruch oder Fehler die
konkrete Meldung beachten. Eine notwendige Wiederherstellung der Codex-Datei
verwendet die dabei ausgegebene Sicherung und berücksichtigt spätere Änderungen.

Ein direkter Verbindungstest prüft `initialize`, die Werkzeugliste und
`boot_get_status`. Er bestätigt die Erreichbarkeit der EXE, nicht, dass die
bereits laufende Codex-Sitzung den neuen Server geladen hat. Codex gegebenenfalls
vollständig neu starten und dort die verfügbaren BootPilot-Werkzeuge prüfen.
BootPilot benötigt keinen eigenen KI-API-Schlüssel.

Die neun MCP-Werkzeuge sind `boot_get_status`, `boot_refresh`, `boot_diagnose`,
`boot_plan_order`, `boot_plan_menu`, `boot_plan_cachyos`, `boot_apply_plan`,
`boot_list_backups` und `boot_plan_restore`. Status liest den Cache ohne UAC;
`boot_refresh` ist eine ausdrücklich auszulösende neue Erfassung. Ein lesender
Protokolltest darf nicht mit einer vollständigen Erfassung verwechselt werden.

## Paket ohne Bootzugriff prüfen

```powershell
pwsh -NoProfile -File .\scripts\verify-package.ps1 -PackageRoot 'C:\Program Files\BootPilot'
```

Der Test prüft die vier x64-Programme, startet ausschließlich CLI-Status und den
MCP-Server aus einem leeren temporären Arbeitsverzeichnis und beendet beide.
Er benötigt weder Quellbaum noch Node.js oder Rust. Er startet weder Installer,
Oberfläche noch Administratorhelfer und führt keine Registrierung, Aktualisierung,
Änderung, Wiederherstellung oder Neustartoperation aus.
