# Build the real debug acceptance host with separate embedded frontend assets.
# Ordinary handy.exe and ordinary dist/ are not replaced by this build command.
param()
$ErrorActionPreference = 'Stop'
$taskRepo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
$taskOldConfig = $env:TAURI_CONFIG
$taskOldLocal = $env:LOCALAPPDATA
$taskOldTemp = $env:TEMP
$taskOldTmp = $env:TMP
Push-Location $taskRepo
try {
    $taskFingerprint = & node --input-type=module -e "import { sourceFingerprint } from './tests/agent-harness/stamp.mjs'; console.log(await sourceFingerprint());"
    if ($LASTEXITCODE -ne 0) { throw 'Cannot fingerprint application build inputs' }
    & node node_modules/typescript/bin/tsc --noEmit
    if ($LASTEXITCODE -ne 0) { throw 'Frontend type check failed' }
    & node node_modules/vite/bin/vite.js build --mode agent-harness --outDir tests/agent-harness/.build/frontend
    if ($LASTEXITCODE -ne 0) { throw 'Harness frontend build failed' }
    $env:TAURI_CONFIG = '{"identifier":"com.grain.agent-harness","productName":"Grain Agent Harness","build":{"frontendDist":"../tests/agent-harness/.build/frontend"}}'
    if (-not $env:TMP) { $env:TMP = 'C:\Windows\Temp' }
    Push-Location (Join-Path $taskRepo 'src-tauri')
    try {
        $taskMetadata = (& cargo metadata --format-version 1 --no-deps | ConvertFrom-Json)
        if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
        if ($taskMetadata.target_directory.TrimEnd('\', '/').Length -le 12) {
            Remove-Item Env:LOCALAPPDATA,Env:TEMP -ErrorAction SilentlyContinue
        }
        & cargo build --locked --bin grain-agent-harness --features agent-harness,custom-protocol
        if ($LASTEXITCODE -ne 0) { throw 'Harness backend build failed' }
        $taskMetadata = (& cargo metadata --format-version 1 --no-deps | ConvertFrom-Json)
        $taskBinary = Join-Path $taskMetadata.target_directory 'debug\grain-agent-harness.exe'
        & node (Join-Path $PSScriptRoot 'stamp.mjs') $taskBinary $taskFingerprint
        if ($LASTEXITCODE -ne 0) { throw 'Harness build identity could not be recorded' }
        Write-Output ('Harness executable: ' + $taskBinary)
    } finally { Pop-Location }
} finally {
    $env:TAURI_CONFIG = $taskOldConfig
    $env:LOCALAPPDATA = $taskOldLocal
    $env:TEMP = $taskOldTemp
    $env:TMP = $taskOldTmp
    Pop-Location
}
