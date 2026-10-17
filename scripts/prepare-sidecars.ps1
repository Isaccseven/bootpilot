[CmdletBinding()]
param(
    [ValidateSet('Debug', 'Release')][string]$Profile = 'Release',
    [ValidateSet('x86_64-pc-windows-msvc')][string]$Target = 'x86_64-pc-windows-msvc'
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$OutputEncoding = [Console]::OutputEncoding
. (Join-Path $PSScriptRoot 'package-common.ps1')
if (-not $IsWindows) { throw 'BootPilot packaging requires Windows x64 and the MSVC build tools.' }
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    $metadata = Get-BootPilotBuildMetadata
    $cargoProfile = if ($Profile -eq 'Release') { 'release' } else { 'dev' }
    $folder = $Profile.ToLowerInvariant()
    $destination = Join-Path (Get-Location) 'src-tauri/binaries'
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    foreach ($binary in @('boot-admin', 'boot-cli', 'boot-mcp')) {
        # Explicit bins exclude the test-fixture MCP and development probes.
        & cargo build --locked --target $Target --profile $cargoProfile --package $binary --bin $binary
        if ($LASTEXITCODE -ne 0) { throw "Building $binary failed." }
        $source = Join-Path $metadata.TargetDirectory "$Target/$folder/$binary.exe"
        Assert-WindowsX64Executable $source
        Copy-Item -LiteralPath $source -Destination (Join-Path $destination "$binary-$Target.exe") -Force
    }
    & (Join-Path $PSScriptRoot 'generate-notices.ps1')
    Write-Host "Prepared three production $Profile sidecars for $Target."
} finally { Pop-Location }
