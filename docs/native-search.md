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
explicitly, opens the video instead. The first eight results appear
in the native panel. Use **Load more results** to request another eight,
up to 40, without closing the current player. The helper repeats the
longer prefix using `ytsearch16:`, `ytsearch24:`, etc. rather than a
pagination cursor; loading more may take several seconds, and ordering
may change if YouTube reranks results. Already loaded thumbnail images
are retained by video ID. Click a video row to open its canonical YouTube
URL and play it in the **same** libmpv session. Use **Close** to dismiss search.

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

- Search uses **only local yt-dlp** with bounded `ytsearchN:`
  counts (8, 16, 24, 32, 40) and `--flat-playlist`.
  It does not require Google login, export
  cookies, automatically use third-party proxies, or download
  media files. Search queries are limited to 120 non-control characters.
- Search results contain validated public 11-character video IDs, titles,
  optional channels and duration. Invalid/playlist entries are ignored.
  Results cannot inject arbitrary stream URLs into the player.
- One worker performs search without blocking the Slint UI; outdated
  responses are discarded when a new search or video open supersedes them.
- Offline root tests verify parsing, input validation, empty and malformed
  responses. CI does **not** contact YouTube.
- Full search filters, cursor-based paging (the current Load more
  refetches a longer prefix), provider fallback, accessibility review
  and polished responsive layout remain open before
  #7 acceptance. Public yt-dlp search availability is not guaranteed
  because YouTube's external behavior changes.

For performance, see [issue #30](https://github.com/rteats/yt-lite/issues/30).
The experimentally reduced `--lean-metadata` flag is optional;
normal playback continues to use complete extraction.


## Windows screenshot and playback feedback — 2026-11-23

The user confirmed the native watch metadata (title, channel, duration,
thumbnail), the search results panel, and the ability to search/open/search
again without stopping the current video. They identified three gaps:

1. At 150% DPI, the bottom transport buttons were partially off-screen.
   The original `preferred-height: 640px` window plus taskbar and window
   decorations made the default window too tall for a 1080p screen.
   Controls were also positioned in a stretching vertical layout.
2. The libmpv video image filled the full window behind the 64px header,
   optional 54px metadata row and 106px bottom panel instead of fitting
   **between** the chrome.
3. Search results did not display thumbnail images. The first result
   attempted to play but neither provider returned a *muxed* stream;
   the current selector rejected all separate video/audio formats.
   The user subsequently confirmed successful playback of other results.

### Follow-up implementation on the search branch

- Default window height reduced to **560 logical px** and min-height
  raised to **480 logical px**; bottom controls now occupy a fixed
  116px panel with explicit rows and spacing. Window resizing and
  fullscreen remain enabled.
- libmpv uses dynamic `video-margin-ratio-top/bottom/right` properties to
  letterbox the video into the unobscured center/left area, preserving
  its aspect ratio. When search results open, the right margin also
  reserves space for the 350px results panel, instead of covering the
  picture. Ratios convert Slint logical pixel values using the current
  per-window DPI scale and physical dimensions; resizing, fullscreen,
  search visibility and metadata visibility are handled. Failure logs
  only once.
- Search results now fetch public fixed-origin YouTube JPEG thumbnails
  on separate workers using the existing 512KiB response limit and
  1024×1024 decoded-size limit, then pass image pixels to the native
  list model. Stale responses cannot change newer search rows.
- The player still **prefers muxed HTTP(S) audio/video** formats.
  When none exists, it can select one video-only and one audio-only
  HTTP(S) format with matching request headers, load video, then use
  libmpv's `audio-add` command to attach the audio track inside the
  same session. No FFmpeg mux, separate decoder window, or media
  download occurs. An external-audio command error is actionable.
  End-to-end audible-audio and seek sync still require Windows testing.

The format selector and video margin calculations have deterministic
offline tests. See [draft PR #32](https://github.com/rteats/yt-lite/pull/32).
The separate-track capability also informs the later quality-selection
ticket [#10](https://github.com/rteats/yt-lite/issues/10); this is not
a full implementation of quality/codec selection.

### New Windows smoke test

```powershell
git switch feat/youtube-search-07
git pull --ff-only
cargo test --manifest-path experiments/native-player/Cargo.toml --all-targets
cargo build --release --manifest-path experiments/native-player/Cargo.toml
$helper = (Get-Command yt-dlp).Source
& ".\experiments\native-player\target\release\yt-lite-native-player-proof.exe" --provider auto --yt-dlp $helper --timings
```

Verify that transport buttons remain completely visible at 150% DPI
(initial size and maximized), no part of the video is covered by
top/bottom chrome, and search results load thumbnails. For a video
which used to fail with `No combined audio+video HTTP stream`, verify
**visible video, audible audio and audio/video sync after seeking**.
Also test changing search results repeatedly. The split-stream fallback
is new and has not yet been verified against a real YouTube video.



## Search result depth and automatic quality follow-up — 2026-11-23

The user confirmed that **separate audio/video sync is good**, but
reported that results were always limited to eight and playback image
quality was relatively low.

**Search:** yt-dlp was called with `ytsearch8:` and the JSON parser
also truncated results to eight. The parser now permits up to 40
validated hits and the native **Load more results** button requests
eight additional results at a time. Duplicate upstream search work
is bounded, but this is **not** efficient cursor pagination; it
re-extracts the leading N results each time. A new query resets the
page count and drops stale thumbnail callbacks.

**Automatic playback quality:** previously `select_playback_streams`
always picked a muxed A/V stream when one existed, even if its only
resolution was 360p and separate 1080p video+audio were available.
The policy now prefers a **compatible separate video/audio pair only
when its picture resolution is greater** than the best muxed candidate.
Among separate tracks, it prefers the highest available format at
or below **1080p**; if none exists, it uses the highest available
separate format. A muxed stream remains preferred at equal/better
resolution. The selected resolution and A/V mode are logged **without
any CDN URLs**.

If high-resolution split playback fails and an extracted muxed
stream is available, the same libmpv session retries that combined
format **without re-running extraction**. Only after that fallback
fails does the existing provider/retry logic apply. These changes
keep the bounded in-memory mpv streaming cache, and audio remains
attached with `audio-add` on the same decoder session.

The current metadata adapter exposes resolution, but not yet
codec/bitrate performance preferences. Automatic 1080p is an
initial quality policy, **not** a replacement for the full quality
selector in issue [#10](https://github.com/rteats/yt-lite/issues/10).

### Acceptance for this increment

1. Search for `rick astley`: inspect the initial eight results,
   select **Load more results**, and confirm the list expands
   (16, then 24, up to 40 or exhaustion) without disrupting playback.
2. Open a video offering higher-resolution separate tracks and
   verify the console reports `selected 1080p separate A/V` when
   that format is available. Confirm picture clarity, audio sync,
   seeking and that the video still fits the reserved viewport.
3. Open a video whose highest video-only track cannot be decoded or
   fails to load. Verify that combined A/V fallback is attempted
   when present, instead of leaving the player silent or frozen.
4. Resize, use fullscreen, switch between search results and
   inspect thumbnail loading; no regression from the existing watch
   metadata and search UI is expected.

