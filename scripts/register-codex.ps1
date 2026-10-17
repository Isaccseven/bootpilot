[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$McpPath
)
$ErrorActionPreference = 'Stop'
if ($McpPath -notmatch '^(?:[A-Za-z]:[\\/]|\\\\)') {
    throw 'McpPath muss ein absoluter installierter boot-mcp.exe-Pfad sein.'
}
$taskCliPath = Join-Path ([System.IO.Path]::GetDirectoryName($McpPath)) 'boot-cli.exe'
if (-not (Test-Path -LiteralPath $taskCliPath -PathType Leaf)) {
    throw 'Die installierte boot-cli.exe muss neben boot-mcp.exe liegen.'
}
# One writer: validation, locking, comment preservation and backup live in Rust.
& $taskCliPath register-codex --mcp-path $McpPath --json
if ($LASTEXITCODE -ne 0) {
    throw "BootPilot-Registrierung fehlgeschlagen (Exitcode $LASTEXITCODE)."
}
