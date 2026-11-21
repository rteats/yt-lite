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
