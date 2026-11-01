<#
.SYNOPSIS
Measure Grain Recall diarization through the real worker on Windows.

.DESCRIPTION
Runs the ignored `worker_diarizes_long_analysis_audio` test for each duration
and samples every Grain transcription worker process (`--transcribe-worker`)
while it runs. Reports the worker's peak working set and peak private
(pagefile-backed) commit, which include the analysis buffer, native features
and model weights. The test host process is excluded. GPU memory is not
measured.

Repeated fixture audio stresses memory and throughput; it is not evidence of
speaker-recognition quality.

.EXAMPLE
pwsh scripts/grain_recall_diarize_probe.ps1 -Worker src-tauri/target/debug/handy.exe `
  -Diarizer C:/fixtures/diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf `
  -Audio C:/fixtures/sortformer-2spk-mix.wav -Backend cpu -Minutes 10,60,120
#>
param(
    [Parameter(Mandatory)] [string] $Worker,
    [Parameter(Mandatory)] [string] $Diarizer,
    [Parameter(Mandatory)] [string] $Audio,
    [ValidateSet('cpu', 'vulkan')] [string] $Backend = 'cpu',
    [string] $Minutes = '10',
    [string] $TargetDir
)
$ErrorActionPreference = 'Stop'
# A comma list survives `powershell -File`, which passes arrays as one string.
$durations = $Minutes -split ',' | ForEach-Object { [double]::Parse($_.Trim(), [Globalization.CultureInfo]::InvariantCulture) }
$root = Split-Path -Parent $PSScriptRoot
$runnerScript = (Join-Path $PSScriptRoot 'run-rust-test.ps1').Replace('\', '/')
$runnerConfig = Join-Path ([IO.Path]::GetTempPath()) "grain-recall-probe-$([guid]::NewGuid()).toml"
Set-Content $runnerConfig "[target.x86_64-pc-windows-msvc]`nrunner = ['powershell.exe', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', '$runnerScript']"

$env:HANDY_TRANSCRIBE_WORKER_EXE = (Resolve-Path $Worker).Path
$env:GRAIN_RECALL_TEST_DIARIZER = (Resolve-Path $Diarizer).Path
$env:GRAIN_RECALL_TEST_AUDIO = (Resolve-Path $Audio).Path
$env:GRAIN_RECALL_TEST_BACKEND = $Backend
if ($TargetDir) { $env:CARGO_TARGET_DIR = $TargetDir }
$workerName = [IO.Path]::GetFileNameWithoutExtension($env:HANDY_TRANSCRIBE_WORKER_EXE)

function Get-WorkerIds {
    Get-CimInstance Win32_Process -Filter "Name = '$workerName.exe'" |
        Where-Object { $_.CommandLine -like '*--transcribe-worker*' } |
        ForEach-Object { [int]$_.ProcessId }
}

$preexisting = @(Get-WorkerIds)
try {
    foreach ($minute in $durations) {
        $env:GRAIN_RECALL_TEST_MINUTES = "$minute"
        $log = Join-Path ([IO.Path]::GetTempPath()) "grain-recall-probe-$([guid]::NewGuid()).log"
        $test = Start-Process cargo -PassThru -NoNewWindow -WorkingDirectory (Join-Path $root 'src-tauri') `
            -RedirectStandardError $log -RedirectStandardOutput "$log.out" `
            -ArgumentList @('test', '--locked', '--lib', '--config', $runnerConfig, '--',
                '--ignored', '--nocapture', '--test-threads=1', 'worker_diarizes_long_analysis_audio')
        # Caching the handle keeps the exit code readable after the process ends.
        $null = $test.Handle
        $peakWorkingSet = 0L
        $peakPrivate = 0L
        while (-not $test.HasExited) {
            foreach ($id in Get-WorkerIds) {
                if ($preexisting -contains $id) { continue }
                $process = Get-Process -Id $id -ErrorAction SilentlyContinue
                if ($process) {
                    $peakWorkingSet = [Math]::Max($peakWorkingSet, $process.PeakWorkingSet64)
                    $peakPrivate = [Math]::Max($peakPrivate, $process.PeakPagedMemorySize64)
                }
            }
            Start-Sleep -Milliseconds 100
        }
        $test.WaitForExit()
        $result = Get-Content $log, "$log.out" | Select-String -Pattern '^probe:|test result|panicked'
        [pscustomobject]@{
            Minutes            = $minute
            Backend            = $Backend
            ExitCode           = $test.ExitCode
            PeakWorkingSetMiB  = [Math]::Round($peakWorkingSet / 1MB, 1)
            PeakPrivateMiB     = [Math]::Round($peakPrivate / 1MB, 1)
            Result             = ($result | ForEach-Object { $_.Line.Trim() }) -join ' | '
        }
        Remove-Item $log, "$log.out" -ErrorAction SilentlyContinue
        if ($test.ExitCode -ne 0) { throw "probe for $minute minutes failed (exit $($test.ExitCode))" }
    }
} finally {
    Remove-Item $runnerConfig -ErrorAction SilentlyContinue
}
