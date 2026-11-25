[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)]
    [ValidateSet("Transient","Persistent","External","Edl","Faststart","BufferedAudio","BufferRejected","SeekControl","SeekAudioOnce","SeekAudioAlways","SeekAudioDelay")]
    [string]$Case,
    [ValidateRange(1,65535)]
    [int]$Port = 8771,
    [switch]$Recover
)

$ErrorActionPreference = "Stop"
if ($Recover -and $Case -notlike "Seek*") { throw "-Recover requires a Seek case" }
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$player = (Resolve-Path (Join-Path $root "experiments\native-player\target\release\yt-lite-native-player-proof.exe")).Path
$python = (Resolve-Path (Join-Path $root ".venv-extraction-bench\Scripts\python.exe")).Path
$server = (Resolve-Path (Join-Path $PSScriptRoot "range_fixture.py")).Path
$fixtures = Join-Path $env:TEMP "yt-lite-seek-fixtures"
$video = (Resolve-Path (Join-Path $fixtures "video.mp4")).Path
$audio = (Resolve-Path (Join-Path $fixtures "audio.m4a")).Path

if ($Case -eq "Faststart") {
    $ffmpeg = (Get-Command ffmpeg -ErrorAction Stop).Source
    $fast = Join-Path $env:TEMP "yt-lite-seek-fixtures-faststart"
    New-Item -ItemType Directory -Force $fast | Out-Null
    & $ffmpeg -hide_banner -loglevel error -y -i $video -map 0:v:0 -c copy -movflags +faststart (Join-Path $fast "video.mp4")
    if ($LASTEXITCODE -ne 0) { throw "Faststart video remux failed" }
    & $ffmpeg -hide_banner -loglevel error -y -i $audio -map 0:a:0 -c copy -movflags +faststart (Join-Path $fast "audio.m4a")
    if ($LASTEXITCODE -ne 0) { throw "Faststart audio remux failed" }
    $fixtures = $fast
    $video = Join-Path $fast "video.mp4"
}

$suffix = if ($Recover) { "-recover" } else { "" }
$prefix = Join-Path $root ("network-" + $Case.ToLowerInvariant() + $suffix)
$serverLog = "$prefix-server.log"
$serverErr = "$prefix-server-stderr.log"
$playerLog = "$prefix-player.log"
# Windows Start-Process joins ArgumentList, so quote filesystem arguments.
 $marker = $null
$serverArgs = @("-u", ('"{0}"' -f $server), "--directory", ('"{0}"' -f $fixtures), "--port", "$Port")
if ($Case -eq "Transient") {
    $serverArgs += @("--fail-once", "audio")
} elseif ($Case -eq "Persistent") {
    $serverArgs += @("--fail-always", "audio")
} else {
    $serverArgs += @("--delay-ms", "500")
    if ($Case -eq "BufferedAudio") { $serverArgs += @("--audio-buffer-bytes", "4194304") }
    if ($Case -eq "BufferRejected") { $serverArgs += @("--audio-buffer-bytes", "65536") }
    if ($Case -in @("SeekAudioOnce","SeekAudioAlways","SeekAudioDelay")) {
        $marker = Join-Path $env:TEMP ("yt-lite-seek-arm-" + [guid]::NewGuid().ToString("N"))
        $fault = switch ($Case) { "SeekAudioOnce" { "once" } "SeekAudioAlways" { "always" } "SeekAudioDelay" { "delay" } }
        $serverArgs += @("--seek-marker", ('"{0}"' -f $marker), "--seek-fault", $fault)
    }
}

$process = Start-Process -FilePath $python -ArgumentList $serverArgs -PassThru -WindowStyle Hidden -RedirectStandardOutput $serverLog -RedirectStandardError $serverErr
try {
    $ready = $false
    for ($i = 0; $i -lt 30; $i++) {
        $process.Refresh()
        if ($process.HasExited) { throw "Fixture server exited; inspect $serverErr" }
        if ((Test-Path $serverLog) -and (Select-String -Path $serverLog -Pattern "^range-fixture ready port=$Port " -Quiet)) {
            $ready = $true
            break
        }
        Start-Sleep -Milliseconds 200
    }
    if (-not $ready) { throw "Fixture server did not announce readiness" }
    $response = Invoke-WebRequest -UseBasicParsing -Method Head -Uri "http://127.0.0.1:$Port/video.mp4" -TimeoutSec 5
    if ($response.StatusCode -ne 200) { throw "Fixture HEAD failed" }

    if ($Case -in @("Transient","Persistent")) {
        $playerArgs = @("--startup-http-fixture", "$Port")
        Write-Host "Close the player after startup-fixture done. For Transient, verify audible audio."
    } else {
        $playerArgs = @("--timings","--observed-state","--seek-benchmark","exact","--seek-http-fixture","$Port")
        if ($Case -eq "Edl") { $playerArgs += "--seek-http-edl" }
        if ($Case -like "Seek*") { $playerArgs += "--seek-probe" }
        if ($Recover) { $playerArgs += "--seek-recovery" }
        if ($marker) { $playerArgs += @("--seek-fault-marker", $marker) }
        $playerArgs += $video
        Write-Host "Do not use player controls; close after seek-bench done."
    }
    # Windows PowerShell wraps native stderr as ErrorRecords. Do not terminate
    # this pipeline for ordinary diagnostic messages.
    $ErrorActionPreference = "Continue"
    & $player @playerArgs 2>&1 | Tee-Object -FilePath $playerLog
    $exitCode = $LASTEXITCODE
    $ErrorActionPreference = "Stop"
    if ($exitCode -ne 0) { throw "Player exited with code $exitCode" }
} finally {
    $ErrorActionPreference = "Stop"
    $process.Refresh()
    if (-not $process.HasExited) {
        Stop-Process -Id $process.Id -ErrorAction SilentlyContinue
        $process.WaitForExit()
    }
    $process.Dispose()
    if ($marker) { Remove-Item -LiteralPath $marker -Force -ErrorAction SilentlyContinue }
    Write-Host "Player log: $playerLog"
    Write-Host "Server log: $serverLog"
    Write-Host "Server stderr: $serverErr"
}
