# BootPilot

A Windows desktop app for inspecting boot configuration and preparing explicit,
reviewable boot changes. Built with Tauri 2, React, TypeScript and Rust, with a
local MCP server for Codex.

**Status: under active development. Not release ready.** Planned functionality
below is a roadmap, not a claim that it is implemented or validated on hardware.
There is currently no verified production release or successful hardware boot
test. See the implementation and validation notes accompanying each release.

## Planned scope

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

## Licensing and provenance

BootPilot's original source is licensed under the [MIT License](LICENSE).
Third-party dependencies retain their respective licenses. This project does
not include copied Grub2Win source code, artwork or bootloader binaries.
Grub2Win is an independently licensed project and is not bundled here.
