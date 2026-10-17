[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$OutputEncoding = [Console]::OutputEncoding
if (-not $IsWindows) { throw 'BootPilot release builds require Windows x64 and MSVC.' }
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    & npm ci
    if ($LASTEXITCODE -ne 0) { throw 'npm ci failed.' }
    & npm run build
    if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed.' }
    & (Join-Path $PSScriptRoot 'prepare-sidecars.ps1') -Profile Release
    & cargo build --locked --release --target x86_64-pc-windows-msvc --package bootpilot-app --bin bootpilot
    if ($LASTEXITCODE -ne 0) { throw 'Native application build failed.' }
} finally { Pop-Location }
