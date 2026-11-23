# Native watch shell — issues #5 and #6

**Status:** experimental stacked implementation, not the full client.

This branch builds on the successful direct-yt-dlp Slint/libmpv streaming proof.
It adds an editable native watch URL field, a browser fallback, and a simple
light/dark overlay. The core stays one Slint window and one libmpv playback
session; opening a second video does not spawn a second player.

## Windows smoke test

From the yt-lite repository root:

```powershell
git fetch origin
git switch feat/native-watch-shell-05
git pull --ff-only

cargo test --manifest-path experiments/native-player/Cargo.toml --all-targets
cargo build --release --manifest-path experiments/native-player/Cargo.toml
$player = ".\experiments\native-player\target\release\yt-lite-native-player-proof.exe"
$helper = (Get-Command yt-dlp -ErrorAction Stop).Source

# Launch without a source; enter a URL in the native watch bar:
& $player --provider auto --yt-dlp $helper

# Optional direct startup from a supported public video:
& $player "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --yt-dlp $helper
```

As in the existing native playback proof, a trusted x64
`mpv-2.dll`/`libmpv-2.dll` and all its dependencies must be next
to the native-player executable. yt-dlp and Deno remain separately
installed local helpers; this branch is not a redistributable package.

1. Run **without** a video argument. The native shell should display
   an editable URL field and an idle prompt, rather than an error.
2. Paste a public `https://youtube.com/watch?v=...` or youtu.be link;
   press **Enter** or click **Open**. It should stream with **video,
   audible audio, advancing progress and functioning seeking**.
3. While the first video is playing, enter a different supported watch
   link. The existing player should switch to it without a second
   window or process.
4. Click **Browser**. The *canonical public watch URL* should open
   in the operating system's default browser (never the signed CDN URL).
5. Try invalid `https://youtube.com.evil.example/watch?v=...` and
   malformed text. The status should report an actionable validation
   error without attempting extraction.
6. Toggle **Light/Dark**, resize the window and use existing
   playback controls. Try keyboard navigation (Tab, Enter) and fullscreen.
7. Optionally run a known local MP4 file using the original executable
   argument; local playback should not regress.

Do **not** share `--raw-json` or signed streaming URLs. The standard
console output should not include raw media URLs.

## Light theme contrast and playback latency review (2026-11-21)

The user's Windows smoke test confirmed that the watch window, URL opening,
audio/video playback and basic controls worked. The user also reported two
issues: light mode kept **white text on light widgets**, and yt-dlp/seek
latency appeared higher than the YouTube website.

### Theme fix

The original toggle changed only custom `Rectangle`/ `Text` colors.
Standard Slint `Button` and `LineEdit` widgets have their own
`Palette.color-scheme` and therefore kept the wrong widget contrast.
The toggle now sets `Palette.color-scheme` to `ColorScheme.light` or
`ColorScheme.dark`, and the watch overlay derives its own colors from
that palette. This keeps text, placeholder, enabled/disabled and input
colors consistent with the active widget theme. The user confirmed the light-theme contrast is now readable on
Windows, including the watch-shell controls.

### Optional timing diagnostics

The user-reported delay is an open performance finding, **not disproven**
by successful playback. The native binary supports `--timings`:

```powershell
$exe = ".\\experiments\\native-player\\target\\release\\yt-lite-native-player-proof.exe"
$helper = (Get-Command yt-dlp -ErrorAction Stop).Source
& $exe --provider auto --yt-dlp $helper --timings
```

Paste a public watch URL and seek to several positions. Only these
sanitized timing lines are needed for review:

- `[yt-lite] timing yt-dlp-extract=...ms`: async worker runtime plus
  yt-dlp resolution, before any media is passed to libmpv
- `[yt-lite] timing libmpv-load=...ms`: load command to `FILE_LOADED`;
  *not* necessarily first decoded/displayed frame
- `[yt-lite] timing seek-to-playback-restart=...ms`: seek command
  submission to libmpv's `PLAYBACK_RESTART` event; a useful seek
  latency proxy but not a precise frame-display timestamp

Logs contain neither signed media URLs nor original watch URLs. Do not
share extra raw mpv debug logs or signed stream URLs.

The current yt-dlp extractor launches a local helper and performs
YouTube challenge resolution before libmpv can begin loading the selected
signed media. The browser already has page data and YouTube-specific
streaming machinery. mpv may also have different demuxing and HTTP
seek/cache behavior. Neither difference is yet quantified locally; avoid
claiming a specific bottleneck or tweaking cache sizes until measured.
The prototype **still streams incrementally** and does not predownload
the complete video.

Latency optimization remains an explicit follow-up for the real player;
the feasibility spike established correctness, not YouTube-web
performance parity.

## Measured startup and seek baseline (2026-11-21)

The user rebuilt the branch on Windows: all 7 native-player tests passed,
release compilation passed, and the light theme now has appropriate contrast.

| Playback attempt | yt-dlp extraction | libmpv media load | Combined to `FILE_LOADED` |
| --- | ---: | ---: | ---: |
| 1 | 6,370 ms | 758 ms | 7,128 ms |
| 2 | 5,282 ms | 1,249 ms | 6,531 ms |
| 3 | 7,755 ms | 784 ms | 8,539 ms |
| **Mean** | **6,469 ms** | **930 ms** | **7,399 ms** |

15 seek-to-`PLAYBACK_RESTART` timings: median **164 ms**, mean **277
ms**, 12 of 15 below 250 ms, three slow samples **712, 878, and 990 ms**.
These are event latencies, not exact visible frame presentation times.
The sample does not identify video IDs, cold/warm cache conditions or
matched timings from the official website; do not imply exact parity
or quantify a browser-vs-native difference from this data.

The **yt-dlp extraction stage accounts for about 87%** of the measured
extract+load interval. Optimize it first, while separately investigating
out-of-buffer seeks. Avoid tuning cache or dropping format/caption
capabilities without an A/B measurement.

### Reproducible yt-dlp extraction variants

A PowerShell benchmark compares the existing extractor invocation with
two official, documented but **potentially capability-reducing** options.
It prints only timings and media/caption counts; no raw JSON, signed
stream URLs or metadata are persisted.

```powershell
powershell.exe -NoProfile -File .\experiments\native-player\bench-yt-dlp.ps1 -Repetitions 2
```

Variants:
- `baseline`: the current reliable extractor configuration
- `skip_translated_subs`: `youtube:skip=translated_subs`
- `skip_manifests`: `youtube:skip=hls,dash,translated_subs`

**Do not enable a variant in the actual player based only on a faster
benchmark number.** Check that at least one muxed audio+video format
remains; verify audible playback and seeking on multiple public videos.
Skipping translated subtitles disables potential caption capabilities,
and skipping manifests can eliminate formats, especially on live/HLS
sources. The full extractor remains the default until a measured
optimization preserves required user-facing behavior.

Tracked in [issue #30](https://github.com/rteats/yt-lite/issues/30).

## First yt-dlp extraction A/B results (2026-11-22)

The user ran `bench-yt-dlp.ps1 -Repetitions 2` on their Windows
machine. Each variant used the same public video, and no media was
downloaded. Times below are **metadata extraction only**, not playback
startup. The script ran variants in a fixed order, so warm cache,
network variation, and upstream variability could affect timings.

| Variant | Run 1 | Run 2 | Mean | Media formats | Muxed | Video-only | Audio-only | Auto-caption entries |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Baseline | 5,576 ms | 5,043 ms | **5,309.5 ms** | 43 | 1 | 37 | 5 | 1,104 |
| Skip translated subs | 5,506 ms | 6,516 ms | 6,011 ms | 43 | 1 | 37 | 5 | 1,104 |
| Skip HLS/DASH manifests and translated subs | 4,855 ms | 6,464 ms | 5,659.5 ms | 28 | 1 | 22 | 5 | 1,099 |

Against the baseline, skipping translated subtitles averaged **701.5 ms
slower** (~13.2%), with no count differences on this video; skipping
manifests averaged **350 ms slower** (~6.6%) and lost **15 out of 43
media entries (~35%)**, all from the video-only count, plus five
automatic-caption format entries. One faster individual run with
manifest skipping is not consistent evidence of improvement.

**Decision:** do **not** enable either skip option as the default.
The existing yt-dlp invocation is preserved. Both skip variants require
broader randomized/repeated tests and real-playback validation before
they could be considered as opt-in experiments, especially for
live/HLS, captions and multi-track quality selection. Two repeated
runs on a single public URL are too few to establish statistically
reliable speed differences.

The next latency work should measure cold/warm yt-dlp subprocess,
network/extractor challenge stages and end-to-end first-frame timing,
as well as seek behavior inside vs outside the buffered range. The
current proven incremental streaming and memory bounds remain intact.
See [issue #30](https://github.com/rteats/yt-lite/issues/30).

## Startup phase diagnostic (issue #30)

The previous extraction-variant A/B experiment found **no repeatable speed
advantage** and manifest skipping lost 15 selectable media formats. The
full-featured yt-dlp extractor remains the default.

A new standalone PowerShell profiler separates a **`--version` command
runtime proxy** from the time yt-dlp takes to resolve a watch URL. It
watches a small allowlist of yt-dlp verbose event *categories* in memory:

- First debug line (an approximate initialization signal)
- First logged webpage fetch announcement
- First logged player API request announcement
- First tagged JavaScript solver message (a generic list of available challenge providers is not counted)
- First logged manifest fetch announcement
- Number of observed API and JavaScript messages
- Total extraction elapsed time, playable media-format count and combined
  audio+video format count

It neither prints raw verbose messages nor saves metadata, signed CDN
URLs, cookies or video files. Each video is specified as an **11-character
public YouTube video ID**, not an arbitrary URI or shell argument. The
`--version` measurement is **not pure process spawn cost**: it includes
executable/runtime initialization and may skip normal extractor loading.

From a Windows PowerShell session in the repository root:

```powershell
git switch feat/native-watch-shell-05
git pull --ff-only

# Local offline validation; does not launch yt-dlp.
.\experiments\native-player\profile-yt-dlp.ps1 -SelfTest

# Two measured passes of the public test video.
.\experiments\native-player\profile-yt-dlp.ps1 -Repetitions 2

# For a second public test video, pass only its 11-character ID.
.\experiments\native-player\profile-yt-dlp.ps1 -VideoId VIDEO_ID_11 -Repetitions 2
```

The last example uses a placeholder; substitute a valid ID. If script
execution is restricted by local PowerShell policy, launch with
`powershell.exe -NoProfile -File .\experiments\native-player\profile-yt-dlp.ps1`,
under the user's own configured execution policy.

**Interpretation:** each milestone timestamp is when a message was
*observed*, usually at the start of a request or challenge step. The
difference between two milestones includes any network, local
processing and omitted work; it cannot identify exact DNS, TLS,
network, EJS runtime or subprocess costs. The `--verbose` flag itself
adds overhead and is intentionally **not** part of the production
extractor invocation.

Compare `VersionMs`, `FirstDebugMs` and `ExtractMs` across attempts.
Missing milestones are reported as blanks rather than assigned fake zeroes.
If the majority of time occurs after the first webpage/API announcement,
investigate requests/challenges with controlled experiments. If version
runtime is unusually high, investigate packaging and process launch
without conflating the two kinds of work.

For evidence before a code change, use at least two videos, alternate
their test order, and confirm any optimization retains muxed A/V,
required formats/captions, and actual streaming/seek behavior. Issue
[#30](https://github.com/rteats/yt-lite/issues/30) tracks those gates.

## Two-video startup phase measurements (2026-11-22)

The user ran `profile-yt-dlp.ps1 -Repetitions 2` against the original
test video (`dQw4w9WgXcQ`) and a second public video
(`fq-Xn7ZOZtg`). The successful outputs each reported at least one
combined audio/video stream, and the shown media counts were 43.

| Marker, elapsed from process start | Original video mean | Second video mean |
| --- | ---: | ---: |
| `yt-dlp --version` wall time (separate process) | 1,589 ms | 1,496 ms |
| First debug announcement | 998 ms | 1,007 ms |
| First webpage announcement | 1,437 ms | 1,410 ms |
| First player API announcement | 2,478 ms | 2,584 ms |
| First tagged JS solver message | 3,040 ms | 3,224 ms |
| First manifest announcement | 4,817 ms | 4,989 ms |
| **Total extraction** | **5,914 ms** | **8,017 ms** |
| **After first manifest announcement** | **1,098 ms** | **3,028 ms** |

The second video took about **2,103 ms longer** overall, even though
its first manifest announcement came only about 173 ms later. The
roughly **1,930 ms extra elapsed after that announcement** accounts
for most of the observed difference. It is not necessarily "manifest
download time": this interval may contain subsequent manifest fetches,
network/JavaScript work, or final parsing. No per-request completion
times are available.

The `--version` measurement is not pure process creation cost and
must **not** be subtracted from extraction (different code paths).
The four JS signal messages and two player API messages per run are
event counts, not separate network request totals or evidence of
exact EJS CPU time. The two videos were not alternated in test order.

### Next diagnostic: last manifest marker

The profiler now reports additional, non-sensitive fields:

- `ManifestSignals`: number of manifest messages recognized
- `LastManifestMs`: timestamp of the last recognized manifest
  announcement
- `AfterLastManifestMs`: total time remaining after that message
- `LastJscMs`: last recognized tagged JS solver message

If `LastManifestMs` is much later on the second video, repeated
manifest work may explain the larger tail. If it is similarly early on
both videos, the remaining gap is downstream of the logged manifest
announcements. These are still **observations**, not exact request
durations. Keep the currently working extractor and bounded streaming
configuration unchanged.

```powershell
git switch feat/native-watch-shell-05
git pull --ff-only
powershell.exe -NoProfile -File .\experiments\native-player\profile-yt-dlp.ps1 -Repetitions 2
powershell.exe -NoProfile -File .\experiments\native-player\profile-yt-dlp.ps1 -VideoId fq-Xn7ZOZtg -Repetitions 2
```

No raw URL, cookie, signed media link or unfiltered yt-dlp debug log
is needed. The performance follow-up is
[issue #30](https://github.com/rteats/yt-lite/issues/30).

## Last-manifest timing follow-up (2026-11-23)

The user reran both videos with the extended profiler. Each video once again
reported **one manifest announcement**, four JavaScript solver signals,
two player API signals, 43 media formats and a combined audio/video format.
There is no evidence of repeated *logged* manifest downloads on either
video.

| Measurement (two-run mean) | `dQw4w9WgXcQ` | `fq-Xn7ZOZtg` |
| --- | ---: | ---: |
| Total yt-dlp extraction | 4,886 ms | 7,780.5 ms |
| Last manifest announcement | 3,971.5 ms | 4,710 ms |
| Remaining after last manifest | **914.5 ms** | **3,070.5 ms** |
| Manifest announcements | 1 | 1 |

The second video's extraction takes ~2,894.5 ms longer, of which
~2,156 ms is **after** the last logged manifest announcement. This
can represent manifest response time, subsequent parsing, finalization,
or other unlogged work; it does not establish a network cause.

### Next controlled protocol comparison

The profiler now prints **`ManifestType`**: `hls` for m3u8,
`dash` for MPD, or `other` if the message cannot be identified.
The yt-dlp benchmark also supports `-ManifestOnly` to compare
skipping just HLS, just DASH, and both, rather than conflating all
manifest types with subtitle translations.

The manifest-only benchmark reverses the order of its four variants
on the second repetition to reduce order effects:

```powershell
git pull --ff-only
powershell.exe -NoProfile -File .\experiments\native-player\profile-yt-dlp.ps1 -Repetitions 2
powershell.exe -NoProfile -File .\experiments\native-player\profile-yt-dlp.ps1 -VideoId fq-Xn7ZOZtg -Repetitions 2

# First video: four extraction variants × two rounds
powershell.exe -NoProfile -File .\experiments\native-player\bench-yt-dlp.ps1 -ManifestOnly -Repetitions 2

# Second video: same variants, same counts/format comparisons
powershell.exe -NoProfile -File .\experiments\native-player\bench-yt-dlp.ps1 -ManifestOnly -Repetitions 2 -Url "https://www.youtube.com/watch?v=fq-Xn7ZOZtg"
```

Do not interpret a faster skip variant as an automatic optimization:
skipping manifest extraction removes available formats and may affect
high-quality, live/HLS and separate AV playback. The production
extractor remains unchanged. The upstream yt-dlp README documents
`youtube:skip=hls,dash` as extractor options.

Follow-up: [issue #30](https://github.com/rteats/yt-lite/issues/30).

## Provider policy

**Automatic native playback** now prefers `yt-dlp`, the only provider
confirmed to play a public video with picture, audio, progress and seek
on the user’s Windows machine. If that extraction or stream fails, the
application may try the Rust route *once*, but the Rust-selected stream
previously failed to load in libmpv (error `-13`); it is not a reliable
primary provider. Explicit `--provider rust` and `--provider yt-dlp`
selections never silently change providers. This is a **player** policy,
not a change to the separate root extractor-harness CLI.

The current media selection supports a single combined/muxed audio+video
HTTP(S) stream. It **streams incrementally through libmpv**, using a
bounded in-memory cache without downloading the entire media file.
Separate high-resolution video/audio track pairing is future work.

## Remaining before closing issues #5 and #6

- Validate latest Slint build and keyboard controls on real Windows.
- Improve responsive layout/scale, accessibility, reduced-motion behavior
  and navigation placeholders; the theme is not yet persisted.
- Add thumbnail/metadata presentation, structured errors and explicit
  recover/retry actions within the app; validate opening the external browser.
- Support independent audio and video formats, manual quality control,
  media URL refresh and better fallback on network interruptions.
- Test multiple real video types and clean-system packaging/licensing.

The PR is intentionally **draft** until these gates are measured.
