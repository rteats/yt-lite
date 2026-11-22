# Diagnostic only: profile yt-dlp subprocess and logged extractor milestones.
# PowerShell 5.1+; no media downloads, raw logs, cookies or signed URLs on disk.
# A milestone means yt-dlp announced an action, NOT that request completion
# or JavaScript challenge duration was directly measured.
[CmdletBinding()]
param(
    [ValidatePattern('^[A-Za-z0-9_-]{11}$')] [string] $VideoId = 'dQw4w9WgXcQ',
    [string] $YtDlp = '',
    [ValidateRange(1, 5)] [int] $Repetitions = 2,
    [switch] $SelfTest
)

$ErrorActionPreference = 'Stop'

function Get-SafeMilestone {
    param([AllowEmptyString()][string] $Line)

    # Parse only known textual categories, never return a raw line or its URL.
    if ($Line -match '(?i)^\[youtube\][^:]*:\s+Downloading webpage') { return 'webpage' }
    if ($Line -match '(?i)^\[youtube\].*Downloading .*player API JSON') { return 'player_api' }
    # A generic [jsc] provider listing does not prove a solver ran.
    if ($Line -match '(?i)\[jsc:[^]]+\]') { return 'javascript' }
    if ($Line -match '(?i)^\[youtube\].*Downloading .*?(m3u8|mpd|manifest)') { return 'manifest' }
    if ($Line -match '(?i)^\[debug\]') { return 'debug' }
    return $null
}

if ($SelfTest) {
    $samples = @(
        @{ Text = '[youtube] SAMPLE12345: Downloading webpage'; Kind = 'webpage' },
        @{ Text = '[youtube] SAMPLE12345: Downloading web player API JSON'; Kind = 'player_api' },
        @{ Text = '[debug] [youtube] [jsc:deno] Using challenge solver'; Kind = 'javascript' },
        @{ Text = '[debug] [youtube] [jsc] JS Challenge Providers: deno'; Kind = 'debug' },
        @{ Text = '[youtube] SAMPLE12345: Downloading m3u8 information'; Kind = 'manifest' },
        @{ Text = '[debug] Loaded 1800 extractors'; Kind = 'debug' },
        @{ Text = 'https://signed.example.test/?token=SECRET'; Kind = $null }
    )
    foreach ($sample in $samples) {
        if ((Get-SafeMilestone $sample.Text) -ne $sample.Kind) {
            throw 'Milestone classification self-test failed.'
        }
    }
    Write-Output 'profile-yt-dlp milestone self-test: passed'
    return
}

if (-not $YtDlp) {
    $YtDlp = (Get-Command yt-dlp -ErrorAction Stop).Source
}
# Ensure ProcessStartInfo uses a concrete executable, not an arbitrary shell.
$executable = (Get-Item -LiteralPath $YtDlp -ErrorAction Stop).FullName
$watchUrl = 'https://www.youtube.com/watch?v=' + $VideoId

for ($round = 1; $round -le $Repetitions; $round++) {
    # This is only a CLI startup proxy. --version may bypass extractor
    # initialization; it is NOT an exact decomposition of extraction time.
    $versionTimer = [Diagnostics.Stopwatch]::StartNew()
    try {
        $null = & $executable --ignore-config --version 2>$null
        $versionOk = ($LASTEXITCODE -eq 0)
    } catch {
        $versionOk = $false
    } finally {
        $versionTimer.Stop()
    }

    $process = New-Object Diagnostics.Process
    $psi = New-Object Diagnostics.ProcessStartInfo
    $psi.FileName = $executable
    # Video ID is constrained to 11 safe characters, so the URL cannot
    # insert shell arguments or command-line switches.
    $psi.Arguments = ('--ignore-config --no-playlist --no-warnings --skip-download ' +
        '--dump-single-json --socket-timeout 10 --verbose -- "' + $watchUrl + '"')
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $process.StartInfo = $psi

    $milestones = @{}
    $jsSignals = 0
    $apiSignals = 0
    $manifestSignals = 0
    $lastManifestMs = $null
    $lastJscMs = $null
    $result = 'failed'
    $formats = $null
    $muxed = $null
    $watch = [Diagnostics.Stopwatch]::StartNew()

    try {
        if (-not $process.Start()) { throw 'Could not start helper' }
        # Drain stdout concurrently to prevent a pipe deadlock. The complete
        # JSON is held in memory only and discarded after counting formats.
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadLineAsync()

        $timedOut = $false
        while ($true) {
            if ($watch.ElapsedMilliseconds -gt 60000) {
                $timedOut = $true
                try { $process.Kill() } catch {}
                break
            }
            if (-not $stderrTask.IsCompleted) {
                [Threading.Thread]::Sleep(10)
                continue
            }
            $line = $stderrTask.GetAwaiter().GetResult()
            if ($null -eq $line) { break }
            $kind = Get-SafeMilestone $line
            if ($kind) {
                $elapsed = [int] $watch.ElapsedMilliseconds
                if (-not $milestones.ContainsKey($kind)) { $milestones[$kind] = $elapsed }
                if ($kind -eq 'javascript') {
                    $jsSignals++
                    $lastJscMs = $elapsed
                }
                if ($kind -eq 'player_api') { $apiSignals++ }
                if ($kind -eq 'manifest') {
                    $manifestSignals++
                    $lastManifestMs = $elapsed
                }
            }
            $stderrTask = $process.StandardError.ReadLineAsync()
        }

        if (-not $process.WaitForExit(2000)) {
            $timedOut = $true
            try { $process.Kill() } catch {}
        }
        $watch.Stop()
        if ($timedOut) {
            $result = 'timeout'
        } elseif ($process.ExitCode -ne 0) {
            $result = 'extractor_failed'
        } else {
            $json = $stdoutTask.GetAwaiter().GetResult()
            try {
                $data = $json | ConvertFrom-Json -ErrorAction Stop
                $media = @($data.formats | Where-Object {
                    $_.url -and (
                        ($_.vcodec -and $_.vcodec -ne 'none') -or
                        ($_.acodec -and $_.acodec -ne 'none')
                    )
                })
                $formats = $media.Count
                $muxed = @($media | Where-Object {
                    $_.vcodec -and $_.vcodec -ne 'none' -and
                    $_.acodec -and $_.acodec -ne 'none'
                }).Count
                $result = if ($muxed -gt 0) { 'muxed_available' } else { 'no_muxed_format' }
            } catch {
                $result = 'malformed'
            }
        }
    } catch {
        $watch.Stop()
        # Do not print exception text: it could contain a provider URL.
        $result = 'error'
        try { if (-not $process.HasExited) { $process.Kill() } } catch {}
    } finally {
        $process.Dispose()
    }

    [pscustomobject]@{
        Round = $round
        VersionMs = if ($versionOk) { [int] $versionTimer.ElapsedMilliseconds } else { $null }
        ExtractMs = [int] $watch.ElapsedMilliseconds
        FirstDebugMs = if ($milestones.ContainsKey('debug')) { $milestones['debug'] } else { $null }
        WebpageMs = if ($milestones.ContainsKey('webpage')) { $milestones['webpage'] } else { $null }
        PlayerApiMs = if ($milestones.ContainsKey('player_api')) { $milestones['player_api'] } else { $null }
        JscSignalMs = if ($milestones.ContainsKey('javascript')) { $milestones['javascript'] } else { $null }
        ManifestMs = if ($milestones.ContainsKey('manifest')) { $milestones['manifest'] } else { $null }
        LastManifestMs = $lastManifestMs
        ManifestSignals = $manifestSignals
        AfterLastManifestMs = if ($null -ne $lastManifestMs) { [int] ($watch.ElapsedMilliseconds - $lastManifestMs) } else { $null }
        LastJscMs = $lastJscMs
        ApiSignals = $apiSignals
        JscSignals = $jsSignals
        Media = $formats
        Muxed = $muxed
        Result = $result
    }
}
