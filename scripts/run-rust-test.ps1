# Cargo runner for the real Windows Tauri lib-test executable. Unit-test links
# omit the app manifest, but Tauri's TaskDialogIndirect import needs Common
# Controls v6. Embed the activation manifest in generated test artifacts only.
# Example (PowerShell, repository root):
# $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER = "powershell.exe -NoProfile -File $PWD/scripts/run-rust-test.ps1"
# cargo test --manifest-path src-tauri/Cargo.toml --lib --locked grain_mcp::
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$TestBinary,
    [Parameter(ValueFromRemainingArguments = $true)][string[]]$TestArguments
)
$ErrorActionPreference = 'Stop'
$resolvedBinary = (Resolve-Path -LiteralPath $TestBinary).Path
if ((Split-Path -Leaf $resolvedBinary) -match '^handy_app_lib-.*\.exe$') {
    $kits = Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -ErrorAction SilentlyContinue
    $kitsRoot = if ($kits -and $kits.KitsRoot10) { $kits.KitsRoot10 } else { Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10' }
    $mtTool = Get-ChildItem -LiteralPath (Join-Path $kitsRoot 'bin') -Directory |
        Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' } |
        Sort-Object { [version]$_.Name } -Descending |
        ForEach-Object { Join-Path $_.FullName 'x64\mt.exe' } |
        Where-Object { Test-Path -LiteralPath $_ } |
        Select-Object -First 1
    if (-not $mtTool) { throw 'Windows SDK mt.exe is required to activate Common Controls v6 for Tauri tests.' }
    $manifest = Join-Path (Split-Path -Parent $PSScriptRoot) 'src-tauri\test.manifest'
    & $mtTool -nologo -manifest $manifest "-outputresource:$resolvedBinary;#1"
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
& $resolvedBinary @TestArguments
exit $LASTEXITCODE
