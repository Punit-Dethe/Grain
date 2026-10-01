# Prepare harmless normal-app consent/restart fixtures from maintained source.
# This writes only a unique temporary directory; it never edits a Grain profile.
param()
$ErrorActionPreference = 'Stop'
$taskFixture = Join-Path $PSScriptRoot 'fixtures\lifecycle'
$taskManifest = Get-Content -LiteralPath (Join-Path $taskFixture 'manifest.json') -Raw | ConvertFrom-Json
$taskManifest.id = 'com.example.native-installation-smoke'
$taskManifest.name = 'Native Installation Smoke'
$taskManifest.description = 'Harmless consent and restart acceptance check'
$taskManifest.contributes.actions = @($taskManifest.contributes.actions | Where-Object { $_.id -eq 'hello' })
if ($taskManifest.contributes.actions.Count -ne 1) { throw 'Maintained hello tool is missing' }
$taskManifest.contributes.actions[0].title = 'Say installation hello'
$taskManifest.PSObject.Properties.Remove('entry')
$taskSource = Get-Content -LiteralPath (Join-Path $taskFixture 'main.js') -Raw
$taskRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('grain-native-installation-smoke-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskRoot | Out-Null
Add-Member -InputObject $taskManifest -NotePropertyName entry_source -NotePropertyValue ($taskSource.Replace('"one"', '"installed-one"'))
$taskPack = @{ manifest = $taskManifest; payloads = @{} }
$taskPath = Join-Path $taskRoot 'native-installation-smoke.grainpack'
[System.IO.File]::WriteAllText($taskPath, ($taskPack | ConvertTo-Json -Depth 20), [System.Text.UTF8Encoding]::new($false))
Write-Output ('Import pack: ' + $taskPath)
Write-Output 'Import through Extensions > Import pack, cancel consent and restart; expect Disabled.'
Write-Output 'Allow and enable once, request Say installation hello, restart and request it again; expect Harness hello (installed-one).'
Write-Output 'Check ordinary dictation/pill and the permission sheet, then uninstall Native Installation Smoke.'
