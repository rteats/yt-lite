# Compare yt-dlp extraction approaches without saving video or raw metadata.
# Windows PowerShell 5.1+; uses the actual local yt-dlp and the public URL.
# Never share extractor JSON, signed CDN URLs, cookies or debug traces.
[CmdletBinding()]
param(
    [string] $Url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
    [string] $YtDlp = "",
    [ValidateRange(1, 5)] [int] $Repetitions = 2
)

$ErrorActionPreference = "Stop"

$uri = $null
if (-not [Uri]::TryCreate($Url, [UriKind]::Absolute, [ref] $uri) -or
    $uri.Scheme -ne "https" -or
    $uri.Host -notin @("youtube.com", "www.youtube.com", "m.youtube.com", "youtu.be")) {
    throw "Use a public HTTPS YouTube or youtu.be video URL."
}
if (-not $YtDlp) {
    $YtDlp = (Get-Command yt-dlp -ErrorAction Stop).Source
}

# Only yt-dlp extractors' documented, reversible options. No cookies,
# login, full downloads, hidden web clients or unsupported speed flags.
$variants = @(
    @{ Name = "baseline"; Extra = @() },
    @{ Name = "skip_translated_subs"; Extra = @("--extractor-args", "youtube:skip=translated_subs") },
    @{ Name = "skip_manifests"; Extra = @("--extractor-args", "youtube:skip=hls,dash,translated_subs") }
)
$common = @(
    "--ignore-config", "--no-playlist", "--no-warnings",
    "--skip-download", "--dump-single-json",
    "--socket-timeout", "10"
)

for ($round = 1; $round -le $Repetitions; $round++) {
    foreach ($variant in $variants) {
        $argsList = @($common) + @($variant.Extra) + @("--", $Url)
        $timer = [Diagnostics.Stopwatch]::StartNew()

        # JSON stays transient in process memory; output contains counts only.
        $raw = (& $YtDlp @argsList 2>$null | Out-String)
        $exitCode = $LASTEXITCODE
        $timer.Stop()

        if ($exitCode -ne 0 -or [string]::IsNullOrWhiteSpace($raw)) {
            [pscustomobject]@{
                Round = $round; Variant = $variant.Name
                ExtractMs = [int] $timer.ElapsedMilliseconds
                Media = $null; Muxed = $null
                VideoOnly = $null; AudioOnly = $null; AutoCaptions = $null
                Result = "failed"
            }
            continue
        }

        try {
            $data = $raw | ConvertFrom-Json
        } catch {
            [pscustomobject]@{
                Round = $round; Variant = $variant.Name
                ExtractMs = [int] $timer.ElapsedMilliseconds
                Media = $null; Muxed = $null
                VideoOnly = $null; AudioOnly = $null; AutoCaptions = $null
                Result = "malformed"
            }
            continue
        }

        $media = @($data.formats | Where-Object {
            $_.url -and (
                ($_.vcodec -and $_.vcodec -ne "none") -or
                ($_.acodec -and $_.acodec -ne "none")
            )
        })
        $muxed = @($media | Where-Object {
            $_.vcodec -and $_.vcodec -ne "none" -and
            $_.acodec -and $_.acodec -ne "none"
        }).Count
        $videoOnly = @($media | Where-Object {
            $_.vcodec -and $_.vcodec -ne "none" -and
            (-not $_.acodec -or $_.acodec -eq "none")
        }).Count
        $audioOnly = @($media | Where-Object {
            $_.acodec -and $_.acodec -ne "none" -and
            (-not $_.vcodec -or $_.vcodec -eq "none")
        }).Count
        $autoCaptionTracks = 0
        if ($null -ne $data.automatic_captions) {
            foreach ($track in $data.automatic_captions.PSObject.Properties) {
                $autoCaptionTracks += @($track.Value).Count
            }
        }

        [pscustomobject]@{
            Round = $round
            Variant = $variant.Name
            ExtractMs = [int] $timer.ElapsedMilliseconds
            Media = $media.Count
            Muxed = $muxed
            VideoOnly = $videoOnly
            AudioOnly = $audioOnly
            AutoCaptions = $autoCaptionTracks
            Result = if ($muxed -gt 0) { "muxed_available" } else { "no_muxed_format" }
        }
    }
}
