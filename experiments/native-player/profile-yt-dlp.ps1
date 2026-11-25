# Diagnostic only: profile yt-dlp subprocess and logged extractor milestones.
# PowerShell 5.1+; no media downloads, raw logs, cookies or signed URLs on disk.
# A milestone means yt-dlp announced an action, NOT that request completion
# or JavaScript challenge duration was directly measured.
[CmdletBinding()]
param(
    [ValidatePattern('^[A-Za-z0-9_-]{11}$')] [string] $VideoId = 'dQw4w9WgXcQ',
    [string] $YtDlp = '',
    [ValidateRange(1, 5)] [int] $Repetitions = 2,
    [string] $NativeEjs = '',
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

# Return a protocol category, never the manifest URL itself.
function Get-SafeManifestType {
    param([AllowEmptyString()][string] $Line)
    if ($Line -match '(?i)\bm3u8\b') { return 'hls' }
    if ($Line -match '(?i)\bmpd\b') { return 'dash' }
    return 'other'
}

# A command announcement is stronger than listing an available runtime,
# but is still not proof of a successful solve. Deno's package-cache probe
# can also start deno; count only launches after the solve announcement.
function Get-ObservedSolver {
    param([AllowEmptyString()][string] $Line)
    if ($Line -match '(?i)\[jsc:quickjs\].*Running QuickJS:') { return 'native_ejs' }
    if ($Line -match '(?i)\[jsc:deno\].*Running deno:') { return 'deno' }
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
    if ((Get-SafeManifestType '[youtube] id: Downloading m3u8 information') -ne 'hls' -or
        (Get-SafeManifestType '[youtube] id: Downloading mpd manifest') -ne 'dash' -or
        (Get-SafeManifestType '[youtube] id: Downloading manifest') -ne 'other') {
        throw 'Manifest protocol classifier self-test failed.'
    }
    if ((Get-ObservedSolver '[debug] [youtube] [jsc:quickjs] Running QuickJS: ejs.exe --script temp.js') -ne 'native_ejs' -or
        (Get-ObservedSolver '[debug] [youtube] [jsc:deno] Running deno: deno run -') -ne 'deno' -or
        $null -ne (Get-ObservedSolver '[debug] [youtube] [jsc] JS Challenge Providers: deno, quickjs')) {
        throw 'Solver launch classifier self-test failed.'
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

$variants = @('default')
$solverPath = $null
if ($NativeEjs) {
    $solverPath = (Get-Item -LiteralPath $NativeEjs -ErrorAction Stop).FullName
    if (-not (Test-Path -LiteralPath $solverPath -PathType Leaf)) { throw 'Native EJS must be a file' }
    $variants = @('deno', 'native_ejs')
}
for ($round = 1; $round -le $Repetitions; $round++) {
    $order = @($variants)
    if ($round % 2 -eq 0) { [array]::Reverse($order) }
    foreach ($variant in $order) {
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
    $runtimeArgs = ''
    if ($variant -eq 'deno') {
        $runtimeArgs = '--no-plugin-dirs --no-js-runtimes --js-runtimes deno '
    } elseif ($variant -eq 'native_ejs') {
        $runtimeArgs = '--no-plugin-dirs --no-js-runtimes --js-runtimes "quickjs:' + $solverPath + '" '
    }
    $psi.Arguments = ('--ignore-config --no-playlist --no-warnings --skip-download ' +
        '--dump-single-json --socket-timeout 10 --verbose ' + $runtimeArgs + '-- "' + $watchUrl + '"')
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $process.StartInfo = $psi

    $milestones = @{}
    $solverObserved = $false
    $denoSolveAnnounced = $false
    $jsSignals = 0
    $apiSignals = 0
    $manifestSignals = 0
    $lastManifestMs = $null
    $lastManifestType = $null
    $lastJscMs = $null
    $result = 'failed'
    $formats = $null
    $muxed = $null
    $formatDigest = $null
    $videoOnly = $null
    $audioOnly = $null
    $autoCaptions = $null
    $chapters = $null
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
            if ($line -match '(?i)\[jsc:deno\].*Solving JS challenges using deno') {
                $denoSolveAnnounced = $true
            }
            $observed = Get-ObservedSolver $line
            if ($variant -eq 'native_ejs' -and $observed -eq 'native_ejs') { $solverObserved = $true }
            if ($variant -eq 'deno' -and $observed -eq 'deno' -and $denoSolveAnnounced) { $solverObserved = $true }
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
                    $lastManifestType = Get-SafeManifestType $line
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
                $videoOnly = @($media | Where-Object { $_.vcodec -and $_.vcodec -ne 'none' -and $_.acodec -eq 'none' }).Count
                $audioOnly = @($media | Where-Object { $_.acodec -and $_.acodec -ne 'none' -and $_.vcodec -eq 'none' }).Count
                $autoCaptions = 0
                if ($null -ne $data.automatic_captions) {
                    foreach ($entry in $data.automatic_captions.PSObject.Properties) {
                        $autoCaptions += @($entry.Value).Count
                    }
                }
                $chapters = if ($null -ne $data.chapters) { @($data.chapters).Count } else { 0 }
                $descriptors = @($media | ForEach-Object {
                    "$($_.format_id)|$($_.vcodec)|$($_.acodec)|$($_.height)"
                } | Sort-Object) -join "`n"
                $sha = [Security.Cryptography.SHA256]::Create()
                try {
                    $formatDigest = [BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($descriptors))).Replace('-', '').Substring(0, 12)
                } finally { $sha.Dispose() }
                $result = if ($muxed -gt 0 -or ($videoOnly -gt 0 -and $audioOnly -gt 0)) { 'media_available' } else { 'no_complete_av' }
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
        Variant = $variant
        SolverObserved = if ($variant -ne 'default') { $solverObserved } else { $null }
        VersionMs = if ($versionOk) { [int] $versionTimer.ElapsedMilliseconds } else { $null }
        ExtractMs = [int] $watch.ElapsedMilliseconds
        FirstDebugMs = if ($milestones.ContainsKey('debug')) { $milestones['debug'] } else { $null }
        WebpageMs = if ($milestones.ContainsKey('webpage')) { $milestones['webpage'] } else { $null }
        PlayerApiMs = if ($milestones.ContainsKey('player_api')) { $milestones['player_api'] } else { $null }
        JscSignalMs = if ($milestones.ContainsKey('javascript')) { $milestones['javascript'] } else { $null }
        ManifestMs = if ($milestones.ContainsKey('manifest')) { $milestones['manifest'] } else { $null }
        LastManifestMs = $lastManifestMs
        ManifestType = $lastManifestType
        ManifestSignals = $manifestSignals
        AfterLastManifestMs = if ($null -ne $lastManifestMs) { [int] ($watch.ElapsedMilliseconds - $lastManifestMs) } else { $null }
        LastJscMs = $lastJscMs
        ApiSignals = $apiSignals
        JscSignals = $jsSignals
        Media = $formats
        Muxed = $muxed
        VideoOnly = $videoOnly
        AudioOnly = $audioOnly
        FormatsSha = $formatDigest
        AutoCaptions = $autoCaptions
        Chapters = $chapters
        Result = $result
    }
    }
}
