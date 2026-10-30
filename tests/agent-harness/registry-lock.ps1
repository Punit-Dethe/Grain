param([Parameter(Mandatory = $true)][string]$Root, [ValidateSet('extensions.json', 'mcp-connections.json', 'grain.settings.json')][string]$FileName = 'extensions.json')
$ErrorActionPreference = 'Stop'
$ownedRoot = [System.IO.Path]::GetFullPath($Root)
$rootItem = Get-Item -LiteralPath $ownedRoot
if ($rootItem.Attributes -band [System.IO.FileAttributes]::ReparsePoint) { throw 'Harness root cannot be a link' }
$markerPath = Join-Path $ownedRoot '.grain-agent-harness.json'
$marker = Get-Content -LiteralPath $markerPath -Raw | ConvertFrom-Json
$runUuid = [guid]::Empty
if ($marker.schema -ne 1 -or -not [guid]::TryParse($marker.runId, [ref]$runUuid)) { throw 'Invalid owned harness marker' }
$dataPath = Join-Path $ownedRoot 'data'
$registryPath = Join-Path $dataPath $FileName
foreach ($target in @($dataPath, $registryPath)) {
    $item = Get-Item -LiteralPath $target
    if ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) { throw 'Registry path cannot be a link' }
}
# Read/write sharing stays permitted; denying DELETE reproduces real Windows
# publication failure without changing bytes or permissions. Never an ordinary profile.
$handle = [System.IO.File]::Open($registryPath, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
try {
    [Console]::WriteLine('HARNESS_REGISTRY_LOCK_READY')
    [Console]::Out.Flush()
    # StreamReader retains asynchronous pipe reading; Console.In is a
    # synchronized reader whose ReadLineAsync can block before returning a task.
    $reader = [System.IO.StreamReader]::new([Console]::OpenStandardInput())
    $release = $reader.ReadLineAsync()
    if (-not $release.Wait(30000)) { throw 'Owned registry lock exceeded 30 seconds' }
} finally {
    $handle.Dispose()
    if ($reader) { $reader.Dispose() }
}
