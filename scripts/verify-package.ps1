[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$PackageRoot)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$OutputEncoding = [Console]::OutputEncoding
if (-not $IsWindows -or -not [IO.Path]::IsPathFullyQualified($PackageRoot)) { throw 'PackageRoot must be an absolute installed or extracted Windows directory.' }
$PackageRoot = (Resolve-Path -LiteralPath $PackageRoot).Path
foreach ($binary in @('bootpilot.exe', 'boot-mcp.exe', 'boot-cli.exe', 'boot-admin.exe')) {
    $file = Join-Path $PackageRoot $binary
    $stream = [IO.File]::OpenRead($file)
    try {
        $reader = [IO.BinaryReader]::new($stream)
        if ($stream.Length -lt 88 -or $reader.ReadUInt16() -ne 0x5a4d) { throw "Invalid executable: $binary" }
        $stream.Position = 0x3c
        $offset = $reader.ReadUInt32()
        if ($offset -gt $stream.Length - 24) { throw "Invalid PE: $binary" }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw "Not Windows x64: $binary" }
        $stream.Position = $offset + 22
        if (($reader.ReadUInt16() -band 0x2002) -ne 2) { throw "Not an executable image: $binary" }
    } finally { $stream.Dispose() }
}
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('bootpilot-package-check-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $scratch | Out-Null
function New-PackageProcess([string]$Binary, [string[]]$Arguments) {
    $info = [Diagnostics.ProcessStartInfo]::new((Join-Path $PackageRoot $Binary))
    $info.WorkingDirectory = $scratch
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.StandardInputEncoding = [Text.UTF8Encoding]::new($false)
    $info.StandardOutputEncoding = [Text.UTF8Encoding]::new($false)
    $info.StandardErrorEncoding = [Text.UTF8Encoding]::new($false)
    foreach ($argument in $Arguments) { $info.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::Start($info)
    $process
}
$cli = $null
$mcp = $null
try {
    # status reads only the existing cache. No refresh, helper or registration.
    $cli = New-PackageProcess 'boot-cli.exe' @('status', '--json')
    $stdout = $cli.StandardOutput.ReadToEndAsync()
    $stderr = $cli.StandardError.ReadToEndAsync()
    $cli.StandardInput.Close()
    if (-not $cli.WaitForExit(15000)) { throw 'Packaged CLI timed out.' }
    if ($cli.ExitCode -ne 0) { throw "Packaged CLI status failed (exit $($cli.ExitCode))." }
    $null = $stdout.GetAwaiter().GetResult() | ConvertFrom-Json
    $null = $stderr.GetAwaiter().GetResult()
    Write-Host 'PASS packaged CLI status --json from an empty working directory.'

    $mcp = New-PackageProcess 'boot-mcp.exe' @()
    $mcpErrors = $mcp.StandardError.ReadToEndAsync()
    function Invoke-PackageMcp([int]$Id, [string]$Method, [hashtable]$Params) {
        $request = @{ jsonrpc = '2.0'; id = $Id; method = $Method; params = $Params } | ConvertTo-Json -Depth 20 -Compress
        $mcp.StandardInput.WriteLine($request)
        $mcp.StandardInput.Flush()
        $deadline = [DateTime]::UtcNow.AddSeconds(15)
        do {
            $remaining = $deadline - [DateTime]::UtcNow
            if ($remaining.TotalMilliseconds -le 0) { throw "MCP $Method timed out." }
            $line = $mcp.StandardOutput.ReadLineAsync().WaitAsync($remaining).GetAwaiter().GetResult()
            if ($null -eq $line) { throw "MCP exited before answering $Method." }
            if ($line.Length -gt 1048576) { throw 'MCP response exceeds the verification bound.' }
            $response = $line | ConvertFrom-Json -AsHashtable
            if ($response.jsonrpc -ne '2.0') { throw 'MCP stdout is not JSON-RPC.' }
        } until ($response.ContainsKey('id') -and $response.id -eq $Id)
        if ($response.ContainsKey('error') -or -not $response.ContainsKey('result')) { throw "MCP rejected $Method." }
        $response.result
    }
    $initialized = Invoke-PackageMcp 1 'initialize' @{ protocolVersion = '2025-03-26'; capabilities = @{}; clientInfo = @{ name = 'bootpilot-package-check'; version = '1' } }
    if ($initialized.serverInfo.name -ne 'bootpilot') { throw 'Unexpected MCP server.' }
    $mcp.StandardInput.WriteLine('{"jsonrpc":"2.0","method":"notifications/initialized"}')
    $mcp.StandardInput.Flush()
    $listed = Invoke-PackageMcp 2 'tools/list' @{}
    $expected = @('boot_get_status', 'boot_refresh', 'boot_diagnose', 'boot_plan_order', 'boot_plan_menu', 'boot_plan_cachyos', 'boot_apply_plan', 'boot_list_backups', 'boot_plan_restore')
    $actual = @($listed.tools | ForEach-Object { $_.name })
    if ($actual.Count -ne 9 -or @(Compare-Object $expected $actual).Count -ne 0) { throw 'The packaged MCP must expose exactly the nine expected tools.' }
    $status = Invoke-PackageMcp 3 'tools/call' @{ name = 'boot_get_status'; arguments = @{} }
    if ($status['isError'] -eq $true -or $status.structuredContent.schema_version -ne 1 -or -not $status.structuredContent.ContainsKey('snapshot')) { throw 'Packaged MCP status failed.' }
    $mcp.StandardInput.Close()
    if (-not $mcp.WaitForExit(5000) -or $mcp.ExitCode -ne 0) { throw 'Packaged MCP did not exit cleanly.' }
    $null = $mcpErrors.GetAwaiter().GetResult()
    Write-Host 'PASS packaged MCP initialize, tools/list (9), boot_get_status and clean exit without source files, Node, refresh or elevation.'
    Write-Host 'No installer, GUI, helper, registration, apply, restore or reboot was started. This is not a hardware boot test.'
} finally {
    foreach ($process in @($cli, $mcp)) {
        if ($null -ne $process) {
            if (-not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
            $process.Dispose()
        }
    }
    $resolvedScratch = [IO.Path]::GetFullPath($scratch)
    if (-not $resolvedScratch.StartsWith([IO.Path]::GetFullPath([IO.Path]::GetTempPath()), [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($resolvedScratch) -notlike 'bootpilot-package-check-*') { throw 'Unsafe verification cleanup path.' }
    Remove-Item -LiteralPath $resolvedScratch -Recurse -Force
}
