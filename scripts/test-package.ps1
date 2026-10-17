[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$OutputEncoding = [Console]::OutputEncoding
. (Join-Path $PSScriptRoot 'package-common.ps1')
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('bootpilot-package-tests-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $scratch | Out-Null
$passed = 0
function Assert-Rejected([scriptblock]$Action, [string]$Name) {
    $rejected = $false
    try { & $Action } catch { $rejected = $true }
    if (-not $rejected) { throw "$Name was accepted unexpectedly." }
    $script:passed++
    Write-Host "PASS $Name"
}
try {
    Assert-Rejected { Assert-PackageOutputDirectory 'relative-output' } 'relative_output_rejected'
    Assert-Rejected { Assert-PackageOutputDirectory ([IO.Path]::GetPathRoot($scratch)) } 'volume_root_rejected'
    $occupied = New-Item -ItemType Directory -Path (Join-Path $scratch 'occupied')
    Set-Content -LiteralPath (Join-Path $occupied 'private.txt') -Value 'preserve me' -Encoding utf8NoBOM
    Assert-Rejected { Assert-PackageOutputDirectory $occupied.FullName } 'nonempty_output_rejected'
    Assert-Rejected { & (Join-Path $PSScriptRoot 'package.ps1') -OutputDirectory $occupied.FullName } 'package_entrypoint_preserves_existing_output'
    if ((Get-Content -Raw -Encoding UTF8 (Join-Path $occupied 'private.txt')).Trim() -ne 'preserve me') { throw 'Existing data changed.' }
    $empty = New-Item -ItemType Directory -Path (Join-Path $scratch 'empty')
    Assert-PackageOutputDirectory $empty.FullName
    Assert-PackageOutputDirectory (Join-Path $scratch 'absent')
    $passed++; Write-Host 'PASS empty_and_absent_outputs_accepted'
    Assert-Rejected { & (Join-Path $PSScriptRoot 'verify-package.ps1') -PackageRoot 'relative' } 'verifier_relative_root_rejected'
    Assert-Rejected { & (Join-Path $PSScriptRoot 'verify-package.ps1') -PackageRoot $empty.FullName } 'verifier_missing_binaries_rejected_before_execution'

    $stage = New-Item -ItemType Directory -Path (Join-Path $scratch 'portable')
    $bytes = [byte[]]::new(512)
    $bytes[0] = 77; $bytes[1] = 90; $bytes[0x3c] = 0x80
    $bytes[0x80] = 80; $bytes[0x81] = 69; $bytes[0x84] = 0x64; $bytes[0x85] = 0x86; $bytes[0x96] = 2
    foreach ($name in @('bootpilot.exe', 'boot-mcp.exe', 'boot-cli.exe', 'boot-admin.exe')) {
        [IO.File]::WriteAllBytes((Join-Path $stage $name), $bytes)
    }
    foreach ($name in Get-PortableTextFiles) {
        $file = Join-Path $stage $name
        New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($file)) -Force | Out-Null
        Set-Content -LiteralPath $file -Value 'test notice' -Encoding utf8NoBOM
    }
    $archive = Join-Path $scratch 'portable.zip'
    [IO.Compression.ZipFile]::CreateFromDirectory($stage, $archive)
    Assert-PortableArchive $archive
    $passed++; Write-Host 'PASS exact_portable_structure_accepted'
    foreach ($name in @('.git/', '../outside/', 'boot-mcp.exe')) {
        $unsafe = Join-Path $scratch ('unsafe-' + [guid]::NewGuid() + '.zip')
        Copy-Item -LiteralPath $archive -Destination $unsafe
        $zip = [IO.Compression.ZipFile]::Open($unsafe, [IO.Compression.ZipArchiveMode]::Update)
        try { $null = $zip.CreateEntry($name) } finally { $zip.Dispose() }
        Assert-Rejected { Assert-PortableArchive $unsafe } "unsafe_or_duplicate_zip_entry_rejected:$name"
    }
    [IO.File]::WriteAllBytes((Join-Path $stage 'boot-mcp-fixture.exe'), $bytes)
    $extra = Join-Path $scratch 'extra.zip'
    [IO.Compression.ZipFile]::CreateFromDirectory($stage, $extra)
    Assert-Rejected { Assert-PortableArchive $extra } 'fixture_binary_rejected'
    Remove-Item -LiteralPath (Join-Path $stage 'boot-mcp-fixture.exe')
    [IO.File]::WriteAllBytes((Join-Path $stage 'boot-mcp.exe'), [byte[]]::new(0))
    $missing = Join-Path $scratch 'empty-binary.zip'
    [IO.Compression.ZipFile]::CreateFromDirectory($stage, $missing)
    Assert-Rejected { Assert-PortableArchive $missing } 'empty_binary_rejected'
    $bad = Join-Path $scratch 'not-pe.exe'
    Set-Content -LiteralPath $bad -Value 'MZ but not executable' -Encoding utf8NoBOM
    Assert-Rejected { Assert-WindowsX64Executable $bad } 'invalid_pe_rejected'
    $bytes[0x84] = 0x4c; $bytes[0x85] = 0x01
    [IO.File]::WriteAllBytes($bad, $bytes)
    Assert-Rejected { Assert-WindowsX64Executable $bad } 'wrong_architecture_rejected'
    $bytes[0x84] = 0x64; $bytes[0x85] = 0x86; $bytes[0x97] = 0x20
    [IO.File]::WriteAllBytes($bad, $bytes)
    Assert-Rejected { Assert-WindowsX64Executable $bad } 'dll_instead_of_executable_rejected'

    $noticeFixture = Join-Path $scratch 'notices'
    $noticeScripts = New-Item -ItemType Directory -Path (Join-Path $noticeFixture 'scripts') -Force
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'generate-notices.ps1') -Destination $noticeScripts.FullName
    $noticeLock = @{ packages = @{
        '' = @{ name = 'fixture' }
        'node_modules/required-runtime' = @{ version = '1.0.0'; license = 'MIT' }
        'node_modules/optional-runtime' = @{ version = '1.0.0'; license = 'MIT'; optional = $true }
        'node_modules/shared-tool' = @{ version = '1.0.0'; license = 'MIT'; devOptional = $true }
        'node_modules/other-platform' = @{ version = '1.0.0'; optional = $true }
        'node_modules/omitted-shared-tool' = @{ version = '1.0.0'; devOptional = $true }
        'node_modules/development-tool' = @{ version = '1.0.0'; dev = $true }
    } }
    $noticeLock | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $noticeFixture 'package-lock.json') -Encoding utf8NoBOM
    foreach ($name in @('required-runtime', 'optional-runtime', 'shared-tool')) {
        $directory = New-Item -ItemType Directory -Path (Join-Path $noticeFixture "node_modules/$name") -Force
        @{ name = $name; version = '1.0.0' } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $directory 'package.json') -Encoding utf8NoBOM
        Set-Content -LiteralPath (Join-Path $directory 'LICENSE') -Value "License text for $name" -Encoding utf8NoBOM
    }
    $runNoticeFixture = {
        # Stub only Cargo's dependency source; exercise the actual npm notice loop and file output.
        function cargo { $global:LASTEXITCODE = 0; '{"packages":[]}' }
        & (Join-Path $noticeScripts 'generate-notices.ps1')
    }
    & $runNoticeFixture
    $passed++; Write-Host 'PASS absent_optional_and_dev_optional_packages_accepted'
    $noticeOutput = Join-Path $noticeFixture 'src-tauri/generated/THIRD-PARTY-LICENSES.txt'
    $notices = Get-Content -LiteralPath $noticeOutput -Raw -Encoding UTF8
    foreach ($name in @('required-runtime', 'optional-runtime', 'shared-tool')) {
        if (-not $notices.Contains("===== npm: $name 1.0.0 =====") -or -not $notices.Contains("License text for $name")) { throw "Installed package notice is missing: $name" }
        $passed++; Write-Host "PASS installed_package_license_included:$name"
    }
    $noticeLock.packages['node_modules/missing-required'] = @{ version = '1.0.0' }
    $noticeLock | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $noticeFixture 'package-lock.json') -Encoding utf8NoBOM
    Assert-Rejected $runNoticeFixture 'missing_required_npm_package_rejected'
    if ((Get-Content -LiteralPath $noticeOutput -Raw -Encoding UTF8) -cne $notices) { throw 'Failed notice generation replaced the previous complete output.' }
    $passed++; Write-Host 'PASS incomplete_notices_do_not_replace_existing_output'
    Write-Host "PASS $passed package guard checks; no fixture executable was run."
} finally {
    $resolvedScratch = [IO.Path]::GetFullPath($scratch)
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if (-not $resolvedScratch.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($resolvedScratch) -notlike 'bootpilot-package-tests-*') { throw 'Unsafe test cleanup path.' }
    Remove-Item -LiteralPath $resolvedScratch -Recurse -Force
}
