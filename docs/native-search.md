# Native YouTube search — first vertical slice (#7)

**Status:** Draft, stacked on the native watch-metadata PR #31 and watch shell
PR #29. Real Windows keyboard, UI and network testing is still required.

## How to run on Windows

This experiment uses the existing local `yt-dlp` helper and colocated
libmpv DLL. It does not bundle those dependencies yet.

```powershell
git fetch origin
git switch feat/youtube-search-07
git pull --ff-only
cargo test --all-targets
cargo test --manifest-path experiments/native-player/Cargo.toml --all-targets
cargo build --release --manifest-path experiments/native-player/Cargo.toml

$player = ".\experiments\native-player\target\release\yt-lite-native-player-proof.exe"
$helper = (Get-Command yt-dlp).Source
& $player --provider auto --yt-dlp $helper --timings
```

Use the existing top field to type a short search phrase (for example
`rust slint tutorial`) and press **Enter** or click **Search**.
Entering a YouTube URL and pressing **Enter**, or clicking **Open**
explicitly, opens the video instead. Up to eight videos should appear
in a right-side native results panel. Click a video row to open its canonical YouTube URL and
play it in the **same** libmpv session. Use **Close** to dismiss search.

## Manual acceptance checks

1. Run a nonempty, public search; result rows should show title, channel,
   and duration when the upstream flat response includes them.
2. Search while a video is already playing. Its audio/video and transport
   controls should continue while the search result panel loads.
3. Open a result, then perform seeking and switch to a second result.
   There should be no extra window or duplicate decoder.
4. Empty or invalid search text displays an actionable state, not a
   media load failure. Search failures should not unload the current video.
5. Test no-result and invalid-url scenarios; theme-switch and fullscreen
   transitions; window at minimum size; keyboard Tab/Enter focus.
6. Existing direct URL input, light/dark themes, metadata header,
   thumbnail, Retry and local MP4 playback must not regress.

## Implementation and scope

- Search uses **only local yt-dlp** with `ytsearch8:` and
  `--flat-playlist`. It does not require Google login, export
  cookies, automatically use third-party proxies, or download
  media files. Search queries are limited to 120 non-control characters.
- Search results contain validated public 11-character video IDs, titles,
  optional channels and duration. Invalid/playlist entries are ignored.
  Results cannot inject arbitrary stream URLs into the player.
- One worker performs search without blocking the Slint UI; outdated
  responses are discarded when a new search or video open supersedes them.
- Offline root tests verify parsing, input validation, empty and malformed
  responses. CI does **not** contact YouTube.
- Full search filters, results thumbnails, paging, provider fallback,
  accessibility review and polished responsive layout remain open before
  #7 acceptance. Public yt-dlp search availability is not guaranteed
  because YouTube's external behavior changes.

For performance, see [issue #30](https://github.com/rteats/yt-lite/issues/30).
The experimentally reduced `--lean-metadata` flag is optional;
normal playback continues to use complete extraction.
