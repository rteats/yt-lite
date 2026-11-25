# Live timing trace and request-level diagnostics (#30)

Run the player with its existing `--timings` switch. The live trace uses one
monotonic process clock and IDs for extraction attempts and user seek requests.
The default extraction and rendering path is otherwise unchanged; tracing
enables verbose yt-dlp and libmpv diagnostics, which may add overhead.

```powershell
git pull --ff-only
cargo build --release --manifest-path experiments/native-player/Cargo.toml
$helper = (Get-Command yt-dlp).Source
$player = ".\experiments\native-player\target\release\yt-lite-native-player-proof.exe"
& $player --provider auto --yt-dlp $helper --timings 2>&1 | Tee-Object -FilePath .\yt-lite-timing.log
```

Reproduce one slow open, several long seeks (including the one that loses
audio), and wait a few seconds while video advances. Keep the complete
trace, not just the summary timings.

- `extract_spawned`, every categorized `extract_marker` with preceding gap,
  first stdout, stdout completion, child exit, and Rust parsing time.
  The gap is not a measured network/solver call duration. Concurrent pipe
  delivery and buffering can shift announcements.
- `load_requested`, libmpv START_FILE/FILE_LOADED, SEEK, PLAYBACK_RESTART,
  AUDIO_RECONFIG/VIDEO_RECONFIG and fixed diagnostic categories. Events
  are observed on the existing 250ms UI timer; this is not exact frame
  presentation or HTTP timing.
- `audio_add_requested`, command reply and first audio PTS observation;
  seek request ID, source/target, restart and audio PTS after restart.
  Overlapping seeks can supersede the prior request. PTS is not proof of
  audible audio. A later event can still refer to a preceding seek.
- `wait_state` snapshots during load/seek/buffering and on state transitions:
  audio clock, seeking, buffering, cache duration/fill/input speed and A/V sync
  when supported. Missing values mean unavailable, not zero. The main
  demuxer cache is not necessarily the separate audio demuxer cache.
- `ui_timer_gap` identifies delayed UI timer callbacks, not the cause.

Only fixed categories, short validated public IDs and numeric properties
are logged by the new trace. Never share a raw yt-dlp verbose or FFmpeg
log, which may contain signed URLs. No cache or format policy is changed.
The audio warning now remains latched during pause/seek and clears only
when audio-clock progress resumes or a new load resets the monitor.

## Optional exact extraction call timings

If the live trace shows a large gap but not the responsible function,
`profile-extractor.py` uses an **already installed Python yt-dlp package**
and times real URL-open calls, response body read/decode, child process
execution and JSON encoding. It is not the standalone exe: compare the
printed package version with `yt-dlp --version`. It hooks internal APIs
which may change, uses normal public extraction with default solver, and
does not persist or print response contents. An HTTP-open span includes
request setup/connection and response acquisition; body-read/decode spans
include transfer and processing. Spans can overlap or nest and should not
be summed as disjoint time. Runtime version checks are distinguished from
other invocations, which can include more than JS solving.

```powershell
python .\experiments\native-player\profile-extractor.py VIDEO_ID
```

The optional script needs Python 3.9+ and yt-dlp installed in that
environment; no automatic installation. Its offline self-test needs no
yt-dlp package. No live network checks are performed by CI.
