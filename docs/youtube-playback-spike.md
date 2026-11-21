# YouTube-to-Slint/libmpv integration spike

**Status:** experimental; not an implementation of the full native watch shell (#5) or URL-playback story (#6). This branch combines the independent proofs in PRs #26 and #27 for a direct Windows smoke test. It must not be merged as a replacement for either standalone proof.

## What it tests

- Existing Slint + libmpv OpenGL window, renderer, pause/seek/resize/fullscreen.
- Existing extraction adapter: try Rust first; if extraction fails or no **muxed audio+video HTTP(S)** format is found, try local yt-dlp.
- A background extraction thread keeps the UI event loop free while resolving the URL; only the UI thread sends commands to the existing libmpv instance.
- The highest-resolution muxed format is loaded as one signed streaming URL. No stream URL, token, or raw extractor output is printed in logs.
- An invalid video link or a local-file mistake is rejected. Existing local-file playback remains available.

**This does not implement:** separate video+audio track pairing, quality selection, read-only media info panels, thumbnails, subtitles, full live recovery, signed-URL refreshing, or forwarding every possible network header. The spike now forwards only User-Agent and Referer from yt-dlp, never cookies or authorization. It now captures asynchronous libmpv load errors but still does not cover every decoder/network failure. These are needed before treating YouTube playback as a finished feature.

## Windows run

From the repository root in PowerShell:

```powershell
git fetch origin
git switch experiment/youtube-native-playback
cargo test --manifest-path experiments/native-player/Cargo.toml --all-targets
cargo build --release --manifest-path experiments/native-player/Cargo.toml
$player = ".\experiments\native-player\target\release\yt-lite-native-player-proof.exe"
$helper = (Get-Command yt-dlp -ErrorAction Stop).Source
& $player "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --yt-dlp $helper
```

The **same trusted x64** `mpv-2.dll`/`libmpv-2.dll` and its dependencies must be next to the built native-player EXE, as in the completed local-file proof. This branch does not download or package native libraries or external helpers. Install yt-dlp and Deno separately if needed; the already successful extraction tests can use those installations. No browser login/cookies are requested.

For regression, also run the known-good local test clip:

```powershell
& $player "C:\Users\user\Downloads\shrek.mp4"
```

Use the actual local file path where needed. Check whether the video *and audible audio* play; test pause, seek and fullscreen after startup. Observe CPU load and frame pacing. **Do not share** raw stream URLs, signed URLs, cookies or `--raw-json` output.

## First real Windows playback observation — 2026-11-21

The user launched the YouTube integration executable with the public video
that passed extraction. The native Slint window rendered the title but the
video surface was black, sound was absent, and the progress remained
`00:00 / 00:00`. **This is a failed playback proof**, despite successful
extraction. The title came from the extraction adapter, not the decoder.

The revised spike now:
- Reads the libmpv event queue (`START_FILE`, `FILE_LOADED`,
  `END_FILE` errors, and asynchronous `loadfile` command replies).
  Previously the code assumed that queuing `loadfile` meant playback.
- Retries via yt-dlp when a Rust-extracted signed URL fails to load,
  including a 30-second load timeout and 20-second no-decoding-progress
  timeout. The second provider is attempted **once**; a second failure is
  shown as an actionable status and a sanitized numeric mpv error.
- Supports `--provider rust`, `--provider yt-dlp`, and `--provider auto`
  for diagnosis (default: `auto`). An explicitly selected provider does
  not silently fall back.
- Clearly displays resolving, connecting, and buffering status instead of
  treating a zero time position as successful playback.

### Streaming, not full-file downloading

The player **streams the selected HTTP(S) media URL through libmpv**.
The yt-dlp helper is used only to resolve metadata and stream addresses
(`--skip-download --dump-single-json`); the player **never invokes yt-dlp
to download media files**. libmpv uses an in-memory cache limited by
`demuxer-max-bytes=16MiB`, `demuxer-max-back-bytes=4MiB`,
`cache-secs=8`, `cache-on-disk=no`. This allows incremental playback
after initial buffering instead of waiting to download the entire video.
The network connection may still fail or buffer while loading.

### yt-dlp request-header compatibility

A selected yt-dlp format can contain request-specific `http_headers`. The
stream parser now reads only `User-Agent` and `Referer`, with format-level
values taking precedence over video-level values, and forwards those through
libmpv's `user-agent` and `referrer` properties before starting HTTP
streaming. Cookies, Authorization and other headers are deliberately omitted,
and request headers/signatures are not written to user-visible logs.

This may resolve HTTP access failures for externally played signed URLs,
but **has not been verified as the root cause** of the original black screen.

### Repeatable troubleshooting

```powershell
git fetch origin
git switch experiment/youtube-native-playback
git pull --ff-only
cargo test --manifest-path experiments/native-player/Cargo.toml --all-targets
cargo build --release --manifest-path experiments/native-player/Cargo.toml
$player = ".\\experiments\\native-player\\target\\release\\yt-lite-native-player-proof.exe"
$helper = (Get-Command yt-dlp -ErrorAction Stop).Source
$url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ"

# Automatically retries yt-dlp if the Rust stream fails to play
& $player $url --provider auto --yt-dlp $helper

# Use these separately to locate a provider-specific network failure:
& $player $url --provider yt-dlp --yt-dlp $helper
& $player $url --provider rust --yt-dlp $helper
```

Observe the **same** x64 mpv DLL setup used for the working local-file
prototype. Report whether the video is visible, audio is audible, progress
advances before the entire file is fetched, and how seeking behaves. If an
error appears, share only the **sanitized** PowerShell lines starting
`[yt-lite]` and the non-URL UI status; do not share signed stream URLs,
raw extractor JSON or full mpv network logs.

## Successful Windows streaming playback — 2026-11-21

User-verified after rebuilding commit `8f143bb9` on Windows 10:

- `cargo test --manifest-path experiments/native-player/Cargo.toml --all-targets`: all 7 integration/action tests passed, with no test failures
- `cargo build --release --manifest-path experiments/native-player/Cargo.toml`: succeeded
- `--provider yt-dlp`: the native Slint/libmpv window played the public YouTube URL **with audible sound**, advancing progress and working seeks; the user reported playback worked well
- The console reported `loading yt-dlp network stream (incremental HTTP)` followed by `yt-dlp media loaded; streaming`
- `--provider auto`: Rust was initially selected, but libmpv returned `mpv media load failed (code -13)`; the fallback switched to yt-dlp and logged `yt-dlp media loaded; streaming`
- Neither the Rust stream's **playability** nor the playback URL's validity is guaranteed by `rusty_ytdl` metadata alone, even when a format is reported as muxed

**Conclusion:** End-to-end extraction, incremental streaming, native decoding,
audible sound, progress, and seeking have now been demonstrated on one public
YouTube watch URL **using yt-dlp**. Automatic fallback was observed reaching
`FILE_LOADED` with yt-dlp after the Rust load failed. The user's explicit
confirmation of audio/progress/seeking applies to the direct yt-dlp run; do
not silently equate the auto fallback's `FILE_LOADED` event with a complete
second audiovisual check.

This is a successful **integration feasibility proof**, not completion of
the watch-shell or general YouTube client. Representative video classes,
actual per-track high-resolution playback, seek/rebuffer behavior, network
interruptions, quality selection, stream URL refresh, and clean packaging
are still outstanding.

The streaming cache remains bounded in RAM and yt-dlp's invocation
does not download media to disk. A network trace was not captured, so
the observed proof confirms playback behavior and implementation design,
not a measured byte-level buffer profile.

## Known limitations and decision gate

The public video in the extraction feasibility test provided one muxed Rust stream and 43 yt-dlp media formats, so this spike deliberately exercises Rust-first playback and avoids the immediate complexity of joining separate audio/video tracks. The currently chosen muxed format might be lower-resolution or require request headers not present in the extraction adapter. If video stays black, spins, plays silently or returns an HTTP error, report the symptom and any *redacted* diagnostics; don't claim the network playback proof succeeded.

The Slint/native proof was validated with **local** MP4, and this integration branch is now validated streaming one public YouTube watch video with **yt-dlp**, including audible sound, progress and seeking. The Rust-selected URL failed with mpv code -13; automatic fallback reached yt-dlp `FILE_LOADED`. Issues #5 and #6 remain the authoritative shell and complete URL-playback milestones.
