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

Replace VERSION with the exact version; if unavailable, document the version difference. Add other public watch URLs as positional arguments to avoid testing only one player script/video. The warm worker's first extraction is cold in terms of connections/solver state; inspect it separately. Cold timings include process startup/import/exit; warm timings exclude shared initialization (reported separately). The results are sanitized summaries, not playback verification. No speedup or connection/solver reuse is assumed.

## Known limitations

A 2.5-second timeout was successful in three runs but never actually triggered a timeout. Skipping the webpage failed in all three attempts and is not implemented. HLS omission was validated for one video only. Connection resets remain unexplained. Audio startup, seeks and UI timer gaps are separate work.
