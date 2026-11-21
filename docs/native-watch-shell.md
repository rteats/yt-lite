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
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\experiments\native-player\bench-yt-dlp.ps1 -Repetitions 2
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
