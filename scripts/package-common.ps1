# Shared build guards. No application, broker or installer is started here.
Set-StrictMode -Version Latest
function Assert-PackageOutputDirectory([string]$Path) {
    if (-not [IO.Path]::IsPathFullyQualified($Path)) { throw 'OutputDirectory must be absolute.' }
    $full = [IO.Path]::GetFullPath($Path).TrimEnd([IO.Path]::DirectorySeparatorChar)
    $root = [IO.Path]::GetPathRoot($Path).TrimEnd([IO.Path]::DirectorySeparatorChar)
    if ($full -eq $root) { throw 'A volume root is not a package output directory.' }
    if (Test-Path -LiteralPath $Path) {
        if (-not (Test-Path -LiteralPath $Path -PathType Container) -or @(Get-ChildItem -LiteralPath $Path -Force).Count -ne 0) {
            throw 'OutputDirectory must be absent or empty; existing data will never be removed.'
        }
        if ((Get-Item -LiteralPath $Path).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'OutputDirectory must not be a reparse point.' }
    }
}
function Assert-WindowsX64Executable([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    try {
        $reader = [IO.BinaryReader]::new($stream)
        if ($stream.Length -lt 88 -or $reader.ReadUInt16() -ne 0x5a4d) { throw "Not a Windows executable: $Path" }
        $stream.Position = 0x3c
        $offset = $reader.ReadUInt32()
        if ($offset -gt $stream.Length - 24) { throw "Invalid PE offset: $Path" }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw "Not a Windows x64 PE: $Path" }
        $stream.Position = $offset + 22
        if (($reader.ReadUInt16() -band 0x2002) -ne 2) { throw "Not an executable image: $Path" }
    } finally { $stream.Dispose() }
}
function Get-PortableTextFiles {
    @('LICENSE', 'README.md', 'THIRD-PARTY-NOTICES.md', 'THIRD-PARTY-LICENSES.txt', 'docs/BENUTZUNG.md', 'docs/ARCHITEKTUR.md', 'docs/PRUEFBERICHT.md', 'docs/CI.md', 'scripts/register-codex.ps1', 'scripts/verify-package.ps1')
}
function Assert-PortableArchive([string]$Path) {
    $allowed = @('bootpilot.exe', 'boot-mcp.exe', 'boot-cli.exe', 'boot-admin.exe') + @(Get-PortableTextFiles)
    $zip = [IO.Compression.ZipFile]::OpenRead($Path)
    try {
        foreach ($entry in $zip.Entries | Where-Object { $_.Name -eq '' }) {
            if ($entry.FullName -cnotin @('docs/', 'scripts/')) { throw 'Portable ZIP contains an unexpected directory.' }
        }
        $files = @($zip.Entries | Where-Object { $_.Name -ne '' })
        if ($files.Count -ne $allowed.Count) { throw 'Portable ZIP file count differs from the explicit allowlist.' }
        foreach ($name in $allowed) {
            $entries = @($files | Where-Object { $_.FullName.Replace('\', '/') -ceq $name })
            if ($entries.Count -ne 1 -or $entries[0].Length -eq 0) { throw "Portable ZIP requires exactly one nonempty $name at its expected path." }
        }
    } finally { $zip.Dispose() }
}
function Get-BootPilotBuildMetadata {
    $json = & cargo metadata --no-deps --locked --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
    $metadata = $json | ConvertFrom-Json
    $versions = @()
    foreach ($name in @('boot-core', 'boot-windows', 'boot-service', 'boot-mcp', 'boot-cli', 'boot-admin', 'bootpilot-app')) {
        $package = @($metadata.packages | Where-Object name -eq $name)
        if ($package.Count -ne 1) { throw "Missing package: $name" }
        $versions += $package[0].version
    }
    if (@($versions | Select-Object -Unique).Count -ne 1) { throw 'Workspace package versions differ.' }
    $config = Get-Content -LiteralPath 'src-tauri/tauri.conf.json' -Raw -Encoding UTF8 | ConvertFrom-Json
    $frontend = Get-Content -LiteralPath 'package.json' -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($config.version -cne $versions[0] -or $frontend.version -cne $versions[0]) { throw 'Tauri, npm and Rust versions differ.' }
    [pscustomobject]@{ Version = $versions[0]; TargetDirectory = $metadata.target_directory }
}
