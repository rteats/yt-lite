# Playback extraction latency experiment

The native-player flag `--fast-extraction` is opt-in. It keeps webpage retrieval, sets yt-dlp's socket timeout to 2.5 seconds, skips HLS, and uses the temporary TV player-JS variant workaround. Standard extraction remains the default. If the fast attempt fails, produces no compatible A/V selection, or the selected media and existing muxed fallback fail in libmpv, the player tries standard yt-dlp once before normal provider fallback. Search, downloads and the public extraction API are unchanged. Do not combine this flag with `--native-ejs` when benchmarking. Remove the temporary variant workaround after validating an upstream release.

## Windows player A/B

Build the release executable using the existing native-player setup (including the bundled mpv DLL). Run the same public video with and without the flag, keeping runtime, helper, metadata mode and network unchanged.

```powershell
cargo build --release --manifest-path experiments/native-player/Cargo.toml
$player = ".\experiments\native-player\target\release\yt-lite-native-player-proof.exe"
$url = "https://www.youtube.com/watch?v=xu1wRfUHtKg"
& $player --provider yt-dlp --yt-dlp $helper --timings $url
& $player --provider yt-dlp --yt-dlp $helper --timings --fast-extraction $url
```

Repeat alternating modes on several public prerecorded videos. Test startup, actual audio/video playback, and seeking. Include failures and fallback duration; don't publish signed URLs or raw provider output. An exit code or a format count alone doesn't prove playback. The fast mode bundles the three previously tested options and does not isolate their individual effects.

## Standalone persistent-worker experiment

Use a disposable Python environment and install the same yt-dlp release as the helper (check `& $helper --version`). Ensure Deno and the matching EJS distribution are installed per yt-dlp's official EJS guide. This benchmark compares cold Python subprocess execution with repeated calls to one YoutubeDL instance using identical options, not the packaged Windows executable. It does not integrate Python into the native player.

```powershell
py -m venv .venv-extraction-bench
.\.venv-extraction-bench\Scripts\python.exe -m pip install "yt-dlp[default]==<VERSION>"
.\.venv-extraction-bench\Scripts\python.exe experiments/extraction-latency/benchmark.py --runs 5 $url
```

Replace VERSION with the exact version; if unavailable, document the version difference. Add other public watch URLs as positional arguments to avoid testing only one player script/video. The warm worker's first extraction is cold in terms of connections/solver state; inspect it separately. Cold timings include process startup/import/exit; warm timings exclude shared initialization (reported separately). The summary counts direct audio and video candidates, not compatible pairs or verified playback; the warm call is not subject to the cold subprocess’s 60-second outer deadline. The results are sanitized summaries, not playback verification. No speedup or connection/solver reuse is assumed.

## Known limitations

A 2.5-second timeout was successful in three runs but never actually triggered a timeout. Skipping the webpage failed in all three attempts and is not implemented. HLS omission was validated for one video only. Connection resets remain unexplained. Audio startup, seeks and UI timer gaps are separate work.


## Persistent native-player integration

The opt-in `--persistent-extraction` mode starts a Python worker asynchronously and uses the same fast arguments as the benchmark. It is independent of the standalone `--fast-extraction` and `--native-ejs` modes. Supply the Python interpreter from the benchmark environment explicitly. The script defaults to `experiments/extraction-latency/worker.py` relative to the current working directory; use `--worker-script PATH` when launching elsewhere.

```powershell
$python = (Resolve-Path ".\.venv-extraction-bench\Scripts\python.exe").Path
$worker = (Resolve-Path ".\experiments\extraction-latency\worker.py").Path
& $player --provider auto --yt-dlp $helper --timings --persistent-extraction --worker-python $python --worker-script $worker
```

Switch between videos in the same window. Compare `persistent_ready`, `persistent_begin`, `persistent_reply`, `persistent_recovery`, `yt-dlp-persistent-extract`, `load_requested` and `audio_pts_first_observed` timings. `wire_ms` includes extraction and serialization; compare with `extract_ms` and `serialize_ms`, rather than treating it as pure IPC. The provisional request budget is 12 seconds, including queue time. A stale in-flight request, timeout, broken pipe or invalid response causes the supervisor to discard that process. A failed or unusable extraction, or media failure through the existing playback recovery path, retries standard yt-dlp once. Python is not bundled yet. Killing the Python process does not guarantee termination of all descendants on Windows; packaging and process-tree supervision remain follow-up work. Run the offline protocol test with `python -m unittest discover -s experiments/extraction-latency -p test_worker.py`.

Offline Rust supervisor test (uses the fake worker, not YouTube):

```powershell
$env:YT_LITE_TEST_PYTHON = $python
cargo test --test persistent_worker -- --ignored
```


## Pre-attached audio startup experiment (issue #30)

The existing split playback flow loads video, observes `FILE_LOADED` via the 250 ms UI timer, then sends `audio-add`. A captured run spent 500 ms until file-loaded, then 1,581 ms until the first audio PTS (2,081 ms total). The optional `--preattach-audio` mode supplies a per-file audio option in the initial libmpv loadfile command, using mpv >= 0.38's index and options arguments. See https://mpv.io/manual/stable/#command-interface-loadfile and https://mpv.io/manual/stable/#options-audio-files . It removes the application-level serial command dependency but does not guarantee that mpv demuxer requests overlap. No actual speedup is claimed. On startup errors or missing audio PTS five seconds after file-loaded (unless paused), retry the same split stream through the existing serial path before muxed/provider fallback. After audio PTS appears, ordinary audio continuity handling applies. Default, muxed playback, quality selection, cache limits and seeking are unchanged. The prior stale audio command-reply fix remains and loadfile command replies also have unique IDs; this does not fully correlate all mpv file lifecycle events.

Build and compare alternating runs in separate sessions, holding extractor settings fixed. For example, append `--preattach-audio` to the existing persistent-mode command, then compare against that command without the flag. Record the same video IDs, selected format labels, `load_requested`, `libmpv-load`, `audio_pts_first_observed since_load_ms`, any `preattached_audio_fallback`, seek restart and audio-clock recovery. FILE_LOADED is not comparable between modes by itself; use time to first audio PTS and visually confirm initial audible audio, frame pacing and seek recovery. Also test rapid replacement and muxed-only media. No new media download or URL logging. The command-seam tests can run offline with `cargo test --manifest-path experiments/native-player/Cargo.toml --test preattached_audio`.


## Controlled seek benchmark (issue #30)

Windows pre-attached audio validation (one run per mode, same 399/251 formats): serial load-to-first-audio-PTS 2723 ms, pre-attached 1817 ms; FILE_LOADED 1500/1503 ms. Pre-attached first audio PTS 0.006 s vs 0.927 s serial; no fallback. One seek from 2.48 s to 20.586 s had 45 us dispatch and 1804 ms until observed restart/audio PTS. Main cache duration was near zero while seeking. User-reported tests passed: Python protocol 1, Rust worker fixture 1, native player 24 (one libmpv-dependent test ignored). No broad playback or warm reuse validation in these two startup runs.

Run `--seek-benchmark exact` or `--seek-benchmark keyframes` with a startup watch URL at least 60 seconds long. These are opt-in diagnostics, not changed default seek policy. Both issue the same six targets (55%, repeated 55%, 25%, repeated 25%, 75%, repeated 75%) after 2.5 seconds of settled unpaused playback with a synchronized audio clock. Do not touch player controls while the benchmark runs. Close the window after `seek-bench done ... status=complete` (it will not exit automatically). A playback error, video replacement or 25-second seek deadline terminates the sequence. A startup/settle deadline is 90 seconds. The UI timer is 50 ms in both benchmark variants, 250 ms normally; observation overhead is not exact frame presentation timing. Keep extraction mode, preattachment, URL and cache configuration fixed across comparisons.

Each request logs pre-seek main-demuxer seekable ranges and `main_cache=inside|outside|unknown`; repeated targets are not assumed cached. Each result logs observed SEEK and PLAYBACK_RESTART times, A/V-ready observation (restart seen, no pause/seeking/cache pause and video/audio position difference <=0.5 s), target and observed landing error. Keyframe seeking may land off-target; observation includes playback progress and UI scheduling. No audible-output or visible-frame verification implied. Main-demuxer ranges do not describe separate audio cache. See https://mpv.io/manual/stable/#command-interface-seek and https://mpv.io/manual/stable/#command-interface-demuxer-cache-state . No signed media URLs or raw provider output logged. Compare matching sample numbers AND cache membership; do not average mismatched cache conditions as if controlled. Use two or more URLs, alternate mode order, keep failed/timeout samples. No cache defaults changed. Cache expansion or range/network instrumentation is a later controlled experiment if warranted.

Offline command/cache classification tests: `cargo test --manifest-path experiments/native-player/Cargo.toml --test seek_benchmark`. Live benchmark requires Windows/libmpv and is not CI-deterministic.
