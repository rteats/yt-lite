# YouTube-to-Slint/libmpv integration spike

**Status:** experimental; not an implementation of the full native watch shell (#5) or URL-playback story (#6). This branch combines the independent proofs in PRs #26 and #27 for a direct Windows smoke test. It must not be merged as a replacement for either standalone proof.

## What it tests

- Existing Slint + libmpv OpenGL window, renderer, pause/seek/resize/fullscreen.
- Existing extraction adapter: try Rust first; if extraction fails or no **muxed audio+video HTTP(S)** format is found, try local yt-dlp.
- A background extraction thread keeps the UI event loop free while resolving the URL; only the UI thread sends commands to the existing libmpv instance.
- The highest-resolution muxed format is loaded as one signed streaming URL. No stream URL, token, or raw extractor output is printed in logs.
- An invalid video link or a local-file mistake is rejected. Existing local-file playback remains available.

**This does not implement:** separate video+audio track pairing, quality selection, read-only media info panels, thumbnails, subtitles, live recovery, signed-URL refreshing, network header forwarding, or error capture from the asynchronous libmpv decode/load stage. These are needed before treating YouTube playback as a finished feature.

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

## Known limitations and decision gate

The public video in the extraction feasibility test provided one muxed Rust stream and 43 yt-dlp media formats, so this spike deliberately exercises Rust-first playback and avoids the immediate complexity of joining separate audio/video tracks. The currently chosen muxed format might be lower-resolution or require request headers not present in the extraction adapter. If video stays black, spins, plays silently or returns an HTTP error, report the symptom and any *redacted* diagnostics; don't claim the network playback proof succeeded.

The Slint/native proof has been validated with **local** MP4; this branch has **not** yet been validated playing a live YouTube stream. Issues #5 and #6 remain the authoritative shell and full URL-playback milestones.
