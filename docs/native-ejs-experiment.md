# Native EJS solver experiment (issue #30)

**Status:** optional experiment, not a production dependency or a measured
end-to-end speedup. Existing extraction, search, playback quality and libmpv
settings remain the defaults.

## Why this implementation

The existing `ahaoboy/ytdlp-ejs` project uses SWC for native AST processing
and embedded QuickJS to execute transformed YouTube player code. Its README
reports 82.227 seconds for 316 fixture cases vs 205.101 seconds using Deno.
These are author-reported Ubuntu solver-only results, not matched Windows
watch startup measurements.

The pinned upstream revision `11fdb193d7be383fd0bb26c00e20faa1ca281263`
contains the QuickJS-compatible `--help` and `--script` interface used by
yt-dlp. We build the `cli,qjs` feature set, not a rewrite or a plugin fork.

Primary sources:

- https://github.com/ahaoboy/ytdlp-ejs/tree/11fdb193d7be383fd0bb26c00e20faa1ca281263
- https://github.com/ahaoboy/ytdlp-ejs/blob/11fdb193d7be383fd0bb26c00e20faa1ca281263/src/main.rs
- https://github.com/yt-dlp/yt-dlp/blob/master/yt_dlp/extractor/youtube/jsc/_builtin/quickjs.py
- https://github.com/yt-dlp/yt-dlp/blob/master/yt_dlp/extractor/youtube/jsc/_builtin/deno.py

## Windows build and test

From yt-lite repository root:

```powershell
git switch feat/youtube-search-07
git pull --ff-only
$ejsRoot = Join-Path $PWD.Path 'experiments\native-player\target\native-ejs'
cargo install --git https://github.com/ahaoboy/ytdlp-ejs --rev 11fdb193d7be383fd0bb26c00e20faa1ca281263 --no-default-features --features cli,qjs --root $ejsRoot ytdlp-ejs
$ejs = Join-Path $ejsRoot 'bin\ejs.exe'
& $ejs --help
cargo build --release --manifest-path experiments/native-player/Cargo.toml

$helper = (Get-Command yt-dlp).Source
$player = ".\experiments\native-player\target\release\yt-lite-native-player-proof.exe"
& $player --provider auto --yt-dlp $helper --native-ejs $ejs --timings
```

The one-time Cargo install downloads/builds third-party source in the user's
environment. Upstream does not commit a Cargo.lock, and one dependency is
Git-based; the source revision is pinned but this is **not** a fully
reproducible dependency lock. Only run a third-party executable you trust.
The process boundary is not a security sandbox. No automatic install,
dependency addition to yt-lite, or persistent signed-URL cache is introduced.
A native solver attempt explicitly uses `--no-plugin-dirs --no-js-runtimes
--js-runtimes quickjs:<path>`; ordinary yt-dlp behavior is unchanged when
the flag is absent.

The app reports `yt-dlp-native-ejs-requested-extract` when the experimental
mode was requested and yielded usable metadata; it does not prove a
challenge ran. It retains complete metadata unless the existing independent
lean flag is explicitly chosen. If extraction errors or yields no playable
A/V, ordinary yt-dlp is attempted once. If the experimental media URLs fail
in playback, the app first tries any available combined A/V and then resolves
with the ordinary yt-dlp runtime before the existing provider fallback.
An individual helper attempt retains the 45-second timeout; combined
retries can exceed it. No guarantee is made about solver descendant process
termination or sandboxing. A successful extraction is not proof that every
available format was recovered.

## Matched measurement (one diagnostic, two videos)

```powershell
powershell.exe -NoProfile -File .\experiments\native-player\profile-yt-dlp.ps1 -NativeEjs $ejs -Repetitions 3
powershell.exe -NoProfile -File .\experiments\native-player\profile-yt-dlp.ps1 -NativeEjs $ejs -VideoId fq-Xn7ZOZtg -Repetitions 3
```

The profiler forces either Deno or the native solver (no fallback and
no plugins) and alternates variant order. It emits only milestones, counts
and non-URL format fingerprints. `SolverObserved=True` indicates a logged
runtime launch, not completion or exact solver CPU duration. A false value
means that run cannot isolate solver performance. The profiler records
full extraction wall time, media-format descriptors, A/V counts, automatic
caption entries and chapter counts. Equal counts and hashes are useful
compatibility checks, not evidence that media URLs work. Verify both
ordinary and native playback with sound, 1080p and seeking. If native
extraction is not faster or loses formats, omit the flag. No global yt-dlp
configuration needs changing.

The 12.3-second seek tail and external-audio load failures from the user's
previous log are tracked separately; this solver experiment cannot resolve
them by itself.
