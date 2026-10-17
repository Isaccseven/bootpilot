# BootPilot

A Windows desktop app for inspecting boot configuration and preparing explicit,
reviewable boot changes. Built with Tauri 2, React, TypeScript and Rust, with a
local MCP server for Codex.

**Status: Windows x64 preview, undergoing final integration and acceptance.**
Unsigned NSIS and portable builds are supported. Automated checks and a direct
MCP handshake do not prove installation, a live Codex session or a successful
hardware boot. Read the [validation report](docs/PRUEFBERICHT.md) and release notes
for the checks actually completed on a particular artifact.

## Scope

- Inspect Windows BCD and UEFI boot entries, boot order and available security
  status, explicitly reporting unavailable data.
- Diagnose Windows/Linux boot configuration using evidence from the system.
- Prepare previews for supported boot order, menu default and timeout changes.
- Manage supported user configuration in an existing Grub2Win installation,
  including validated CachyOS kernel paths.
- Apply only current, validated plans through a narrowly scoped administrator
  helper, with backups, verification and targeted recovery.
- Expose the same structured operations to the desktop UI and a local stdio
  MCP server, using the official Rust MCP SDK.

The initial scope excludes partitioning, formatting, operating system removal,
firmware updates and automatic changes to Secure Boot or BitLocker. No reboot
is performed automatically. Development checks use simulated adapters and
temporary files; a test passing is not proof that a machine will boot.

## Architecture

The UI and MCP server share a Rust core for inventory, diagnosis, plan
validation and recovery. Platform adapters isolate Windows access. Privileged
operations belong in a short-lived helper with defined actions and checked
arguments. The WebView must not have general shell or filesystem access.

Codex provides the assistant; BootPilot does not require its own AI API key.

## Development and contributions

Windows x64 is the initial target. A Rust toolchain, Node.js, the Microsoft C++
build tools and WebView2 are required for Tauri development. Follow the checked-in
package scripts and Rust workspace configuration for the current build commands.
Build and validation instructions should distinguish mocked checks, real
read-only inspection and explicitly authorized hardware changes.

Issues and contributions are welcome. Keep changes focused and document which
behaviors were actually verified. Never include private machine inventories,
boot backups, credentials, recovery material or local toolchains in commits.

With PowerShell 7, Rust 1.99.1, Node.js 24.22.0, MSVC and the Windows SDK on PATH:

```powershell
pwsh -NoProfile -File ./scripts/build.ps1
pwsh -NoProfile -File ./scripts/package.ps1 -OutputDirectory C:\packages\BootPilot
```

The output directory must be absolute and absent or empty. Packaging produces
exactly the versioned NSIS installer and portable ZIP; existing output files are
never deleted. The scripts use the public toolchain PATH, locked dependencies and
an explicit Windows x64 target. They do not install the application, register
Codex, launch the administrator helper, change boot configuration or reboot.
Tauri may download its NSIS tools and WebView2 bootstrapper during packaging.

See [German usage and recovery instructions](docs/BENUTZUNG.md),
[architecture](docs/ARCHITEKTUR.md), [CI and release checks](docs/CI.md), and
[third-party notices](THIRD-PARTY-NOTICES.md).

## Licensing and provenance

BootPilot's original source is licensed under the [MIT License](LICENSE).
Third-party dependencies retain their respective licenses. This project does
not include copied Grub2Win source code, artwork or bootloader binaries.
Grub2Win is an independently licensed project and is not bundled here.
