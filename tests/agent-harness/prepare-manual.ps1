# Prepare the remaining ordinary-app shared-listener check from maintained tools.
# No app/profile/credential changes. The user loads this disposable folder in Grain.
param()
$ErrorActionPreference = 'Stop'
$taskFixture = Join-Path $PSScriptRoot 'fixtures\lifecycle'
$taskManifest = Get-Content -LiteralPath (Join-Path $taskFixture 'manifest.json') -Raw | ConvertFrom-Json
$taskManifest.id = 'com.example.native-failure-smoke'
$taskManifest.name = 'Native Failure Smoke'
$taskManifest.description = 'Harmless ordinary-app recovery check after a deliberately oversized native reply'
$taskManifest.contributes.actions = @($taskManifest.contributes.actions | Where-Object { $_.id -in @('hello', 'oversized_raw') })
if ($taskManifest.contributes.actions.Count -ne 2) { throw 'Maintained native test tools are missing' }
foreach ($taskAction in $taskManifest.contributes.actions) {
    if ($taskAction.id -eq 'hello') { $taskAction.title = 'Say test hello' }
    else { $taskAction.title = 'Return a deliberately oversized reply' }
}
$taskRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('grain-native-failure-smoke-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $taskRoot 'dist') | Out-Null
[System.IO.File]::WriteAllText((Join-Path $taskRoot 'manifest.json'), ($taskManifest | ConvertTo-Json -Depth 20), [System.Text.UTF8Encoding]::new($false))
Copy-Item -LiteralPath (Join-Path $taskFixture 'main.js') -Destination (Join-Path $taskRoot 'dist\main.js')
Write-Output ('Load unpacked folder: ' + $taskRoot)
Write-Output 'Tools: Say test hello; Return a deliberately oversized reply. Both require confirmation.'
Write-Output 'After the oversized-reply warning, test ordinary dictation/pill cancel and a fresh Say test hello. Unload this folder afterward.'
