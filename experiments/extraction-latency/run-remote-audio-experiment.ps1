[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)]
    [ValidateSet("Baseline","Cache","Interactive","SeekProbe")]
    [string]$Case,
    [string]$Url = "https://www.youtube.com/watch?v=xu1wRfUHtKg",
    [ValidatePattern("^[a-zA-Z0-9_-]{1,32}$")]
    [string]$Tag = "reference"
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$player = (Resolve-Path (Join-Path $root "experiments\native-player\target\release\yt-lite-native-player-proof.exe")).Path
$python = (Resolve-Path (Join-Path $root ".venv-extraction-bench\Scripts\python.exe")).Path
$worker = (Resolve-Path (Join-Path $PSScriptRoot "worker.py")).Path
$helper = (Get-Command yt-dlp -ErrorAction Stop).Source
$log = Join-Path $root ("remote-audio-" + $Tag + "-" + $Case.ToLowerInvariant() + ".log")
$playerArgs = @("--provider","auto","--yt-dlp",$helper,"--timings","--persistent-extraction","--worker-python",$python,"--worker-script",$worker,"--preattach-audio","--observed-state")
if ($Case -eq "Interactive") {
    $playerArgs += @("--audio-cache","--transport-recovery")
    Write-Host "Try playback, pause/resume, seek, then replace the watch URL several times. Close when finished."
} else {
    $playerArgs += @("--seek-benchmark","exact")
    if ($Case -eq "Cache") { $playerArgs += "--audio-cache" }
    if ($Case -eq "SeekProbe") { $playerArgs += "--seek-probe" }
    Write-Host "Do not touch playback controls; close after seek-bench done."
}
$playerArgs += $Url
$ErrorActionPreference = "Continue"
& $player @playerArgs 2>&1 | Tee-Object -FilePath $log
$exitCode = $LASTEXITCODE
$ErrorActionPreference = "Stop"
Write-Host "Player log: $log"
if ($exitCode -ne 0) { throw "Player exited with code $exitCode" }
