[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$OutputEncoding = [Console]::OutputEncoding
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    $json = & cargo metadata --locked --filter-platform x86_64-pc-windows-msvc --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Cannot collect locked Cargo dependency notices.' }
    $metadata = $json | ConvertFrom-Json
    $builder = [Text.StringBuilder]::new()
    $null = $builder.AppendLine('BootPilot: locked third-party dependency licenses and notices')
    $null = $builder.AppendLine('Includes Rust build dependencies conservatively. Runtime npm packages only; development tools are not bundled.')
    $null = $builder.AppendLine('Package metadata and original license files follow. Exact published Rust sources are available at the source links.')
    function Add-LicenseFiles([string]$Directory) {
        $files = @(Get-ChildItem -LiteralPath $Directory -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE)' } | Sort-Object Name)
        foreach ($file in $files) {
            $null = $builder.AppendLine("--- $($file.Name) ---")
            $null = $builder.AppendLine([IO.File]::ReadAllText($file.FullName, [Text.Encoding]::UTF8))
        }
        if ($files.Count -eq 0) {
            $null = $builder.AppendLine('No standalone license file is included in this upstream package; consult its SPDX declaration, source headers and source link above.')
        }
    }
    foreach ($package in $metadata.packages | Where-Object source | Sort-Object name, version) {
        $null = $builder.AppendLine("`n===== Rust: $($package.name) $($package.version) =====")
        $null = $builder.AppendLine("License: $($package.license)")
        $null = $builder.AppendLine("Repository: $($package.repository)")
        $null = $builder.AppendLine("Source: https://crates.io/api/v1/crates/$($package.name)/$($package.version)/download")
        Add-LicenseFiles (Split-Path $package.manifest_path -Parent)
    }
    $lock = Get-Content -LiteralPath package-lock.json -Raw -Encoding UTF8 | ConvertFrom-Json -AsHashtable
    foreach ($key in $lock.packages.Keys | Sort-Object) {
        $package = $lock.packages[$key]
        if ($key -eq '' -or $package['dev'] -eq $true) { continue }
        $manifest = Join-Path $key 'package.json'
        if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
            # npm locks optional packages for other platforms too. They are not bundled
            # when absent; installed optional packages still need their notices below.
            if ($package['optional'] -eq $true -or $package['devOptional'] -eq $true) { continue }
            throw "Production npm package is missing: $key"
        }
        $installed = Get-Content -LiteralPath $manifest -Raw -Encoding UTF8 | ConvertFrom-Json
        $null = $builder.AppendLine("`n===== npm: $($installed.name) $($installed.version) =====")
        $null = $builder.AppendLine("License: $($package['license'])")
        $null = $builder.AppendLine("Source: $($package['resolved'])")
        Add-LicenseFiles $key
    }
    $destination = Join-Path (Get-Location) 'src-tauri/generated'
    New-Item -ItemType Directory -Path $destination -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $destination 'THIRD-PARTY-LICENSES.txt'), $builder.ToString(), [Text.UTF8Encoding]::new($false))
    Write-Host 'Generated locked third-party notices (without local toolchain or source paths).'
} finally { Pop-Location }
