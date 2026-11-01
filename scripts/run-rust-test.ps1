# Cargo runner for the real Windows Tauri lib-test executable. Unit-test links
# omit the app manifest, but Tauri's TaskDialogIndirect import needs Common
# Controls v6. Run an isolated copy outside Cargo's deps directory, with the
# sidecar activation manifest; never modify Cargo's cached executable.
# See BUILD.md for the Cargo runner configuration, including paths with spaces.
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$TestBinary,
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$TestArguments
)
$ErrorActionPreference = 'Stop'
$resolvedBinary = (Resolve-Path -LiteralPath $TestBinary).Path
if ((Split-Path -Leaf $resolvedBinary) -notmatch '^handy_app_lib-.*\.exe$') {
    & $resolvedBinary @TestArguments
    exit $LASTEXITCODE
}

$binaryDirectory = Split-Path -Parent $resolvedBinary
$stageParent = if ((Split-Path -Leaf $binaryDirectory) -eq 'deps') {
    Split-Path -Parent $binaryDirectory
} else {
    $binaryDirectory
}
$stageParent = (Resolve-Path -LiteralPath $stageParent).Path
$stageName = 'grain-rust-tests-' + [guid]::NewGuid().ToString('N')
$stagePath = Join-Path $stageParent $stageName
if (Test-Path -LiteralPath $stagePath) { throw 'Test staging directory already exists.' }
New-Item -ItemType Directory -Path $stagePath -ErrorAction Stop | Out-Null
$originalPath = $env:PATH
$testExitCode = 1
try {
    $copy = Join-Path $stagePath 'grain-backend-tests.exe'
    Copy-Item -LiteralPath $resolvedBinary -Destination $copy -ErrorAction Stop
    $manifest = Join-Path (Split-Path -Parent $PSScriptRoot) 'src-tauri\test.manifest'
    Copy-Item -LiteralPath $manifest -Destination "$copy.manifest" -ErrorAction Stop
    # Match the native runtime search paths available to the original test.
    $env:PATH = "$stageParent;$binaryDirectory;$originalPath"
    & $copy @TestArguments
    $testExitCode = $LASTEXITCODE
} finally {
    $env:PATH = $originalPath
    # Delete only this invocation's owned directory, never a sibling test run.
    $stage = Get-Item -LiteralPath $stagePath -ErrorAction Stop
    if (($stage.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
        $stage.Parent.FullName -ne $stageParent -or $stage.Name -ne $stageName) {
        throw 'Refusing cleanup outside the owned test staging directory.'
    }
    Remove-Item -LiteralPath $stage.FullName -Recurse -Force -ErrorAction Stop
}
exit $testExitCode
