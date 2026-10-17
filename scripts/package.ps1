[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$OutputEncoding = [Console]::OutputEncoding
. (Join-Path $PSScriptRoot 'package-common.ps1')
Assert-PackageOutputDirectory $OutputDirectory
if (-not $IsWindows) { throw 'BootPilot packaging requires Windows x64 and MSVC.' }
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    $metadata = Get-BootPilotBuildMetadata
    $version = $metadata.Version
    $target = 'x86_64-pc-windows-msvc'
    & (Join-Path $PSScriptRoot 'build.ps1')
    & npm run tauri -- build --ci --target $target --bundles nsis
    if ($LASTEXITCODE -ne 0) { throw 'Tauri NSIS packaging failed.' }
    $release = Join-Path $metadata.TargetDirectory "$target/release"
    $installer = Join-Path $release "bundle/nsis/BootPilot_${version}_x64-setup.exe"
    if (-not (Test-Path -LiteralPath $installer -PathType Leaf) -or (Get-Item -LiteralPath $installer).Length -eq 0) { throw 'Expected NSIS installer is absent or empty.' }
    $stream = [IO.File]::OpenRead($installer)
    try { if ($stream.ReadByte() -ne 77 -or $stream.ReadByte() -ne 90) { throw 'NSIS output is not an executable.' } } finally { $stream.Dispose() }
    $stage = Join-Path $metadata.TargetDirectory ('bootpilot-package/' + [guid]::NewGuid())
    $portable = Join-Path $stage 'portable'
    New-Item -ItemType Directory -Path $portable -Force | Out-Null
    foreach ($name in @('bootpilot.exe', 'boot-mcp.exe', 'boot-cli.exe', 'boot-admin.exe')) {
        $source = Join-Path $release $name
        Assert-WindowsX64Executable $source
        Copy-Item -LiteralPath $source -Destination (Join-Path $portable $name)
    }
    foreach ($name in Get-PortableTextFiles) {
        $destination = Join-Path $portable $name
        New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($destination)) -Force | Out-Null
        $source = if ($name -eq 'THIRD-PARTY-LICENSES.txt') { 'src-tauri/generated/THIRD-PARTY-LICENSES.txt' } else { $name }
        Copy-Item -LiteralPath $source -Destination $destination
    }
    & (Join-Path $PSScriptRoot 'verify-package.ps1') -PackageRoot $portable
    $archive = Join-Path $stage "BootPilot-$version-windows-x64.zip"
    [IO.Compression.ZipFile]::CreateFromDirectory($portable, $archive, [IO.Compression.CompressionLevel]::Optimal, $false)
    Assert-PortableArchive $archive
    Assert-PackageOutputDirectory $OutputDirectory
    New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
    [IO.File]::Copy($installer, (Join-Path $OutputDirectory "BootPilot-$version-windows-x64-setup.exe"), $false)
    [IO.File]::Copy($archive, (Join-Path $OutputDirectory "BootPilot-$version-windows-x64.zip"), $false)
    Write-Host "Created unsigned BootPilot $version Windows x64 installer and portable package."
} finally { Pop-Location }
