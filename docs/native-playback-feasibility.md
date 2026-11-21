# Native Slint/libmpv rendering proof — issue #2

**Feasibility demonstrated on a user's Windows PC with a local MP4 (2026-11-21).** This remains an experiment, not a release-ready player or a performance benchmark.

## Run locally on Windows 10/11

1. Build the prototype: `cargo build --release --manifest-path experiments/native-player/Cargo.toml`.
2. Download a current `mpv-dev-x86_64-*.7z` libmpv archive from the [Windows builds listed by mpv](https://mpv.io/installation/) (for example, [shinchiro's libmpv files](https://sourceforge.net/projects/mpv-player-windows/files/libmpv/)). Extract the **x64** `libmpv-2.dll` or `mpv-2.dll` from the archive, and place it in `experiments/native-player/target/release/` alongside the EXE. Copy any additional required DLL dependencies from the same trusted package if needed; don't use random DLL-download sites. The experiment deliberately does not redistribute media-engine binaries or their licenses.
3. Use your own non-DRM local 1080p H.264 MP4 test clip, then launch:

```powershell
.\experiments\native-player\target\release\yt-lite-native-player-proof.exe "C:\test\sample.mp4"
```

The crate uses the default, isolated Cargo `target` directory under `experiments/native-player`; adjust the command if you override `CARGO_TARGET_DIR`.

### Troubleshooting `LoadLibraryExW failed`

From the repository root, check which DLL (if any) is beside the EXE:

```powershell
$bin = ".\\experiments\\native-player\\target\\release"
Get-ChildItem $bin -Filter "*mpv*.dll"
```

- **No matching DLL:** the program cannot run yet. Extract the x64 `libmpv-2.dll` from an `mpv-dev-x86_64` archive into `$bin`; do not put it in the repository root.
- **Matching DLL present but loading still fails:** Windows may be missing a dependency of that DLL (or the DLL may have the wrong architecture). Keep the package's required DLLs together; diagnose missing modules with [Dependencies](https://github.com/lucasg/Dependencies) rather than downloading arbitrary DLLs.
- **The clip argument is missing:** provide the full path to an existing MP4 or other supported local video.

After updating the implementation, compile again with `cargo build --release --manifest-path experiments/native-player/Cargo.toml`, then run the EXE with the clip path.

- Seek by +/-10 seconds, click the scrub bar, pause/resume, resize, and toggle fullscreen.
- Inspect video **beneath** the Slint overlay. Verify that none of these operations creates a second player or restarts the clip.
- Compare `hwdec=auto-safe` with `hwdec=no` in this experiment. Check mpv log output or the hardware decoder property before claiming hardware acceleration.
- Measure 24/30/60 fps clips, resize transitions, fullscreen transitions, dropped frames, and overlay responsiveness on Windows 10. Record frame times and display-refresh mismatches.
- On GPU/renderer incompatibility, record failure and try a separate renderer experiment; do not silently switch to a child-window/opaque overlay implementation.

## Architecture

- One process, one Slint `Window`, one mpv core, one mpv render context.
- `RenderingSetup` creates the OpenGL render context; `BeforeRendering` draws video into the current framebuffer; Slint draws a transparent overlay above it; `RenderingTeardown` frees the context.
- Frame notifications enter the Slint event loop through a queued weak component handle, never directly from the mpv callback thread.
- Playback commands are asynchronous, and the control-action mapping is separately tested without a DLL or video asset.
- `libmpv-2.dll` or `mpv-2.dll` is loaded from the **executable directory only**. The prototype intentionally does not download or bundle third-party binaries.
- The Windows WGL + desktop-OpenGL path is selected deliberately to avoid pretending untested ANGLE/Direct3D hardware-interoperability is proven.

## Verification matrix

| Check | Status |
| --- | --- |
| Rust typecheck and action tests on Windows CI | Passed, including progress-bar fix ([run 44926286125](https://github.com/rteats/yt-lite/actions/runs/44926286125)) |
| Native playback and compositing on Windows (user smoke test) | **Pass**: local clip plays in native window, overlay visible, progress fill corrected |
| Playback controls, resize and fullscreen | Resize and fullscreen **pass** user test; progress bar **pass**. Pause and seek were implemented/tested at the command level but not individually reported in user feedback; no unexpected recreation reported |
| Hardware/software decode and frame pacing | Performance and frame pacing **look good subjectively** in user test; no quantitative profiling, dropped-frame data or verified decoder selection |
| Clean-system runtime DLL packaging, licenses | Separate issue #4 |

## Risks / fallback

- Slint FemtoVG and libmpv both use OpenGL state. The render API may modify GL state; verify Slint controls remain correctly drawn after playback and resizes.
- The direct desktop OpenGL path may not expose the best D3D11VA hardware decode zero-copy route. Quantify actual decode and CPU/GPU cost before deciding on production rendering.
- Local property polling uses synchronous mpv property reads on the UI thread. If profiling shows frame stalls, replace with mpv property observation from a separate worker thread. Do **not** set `MPV_RENDER_PARAM_ADVANCED_CONTROL` without ensuring independent renderer/core threads.
- Future fallback candidates are a carefully verified ANGLE render path or a separate video window with Slint overlay handling; neither is implemented here.
- Reference implementations such as Lumiere and Vayou-Slint were researched for architectural feasibility, not copied. The C API signatures are declared against mpv's ISC-licensed public headers. All third-party redistribution conditions remain open until issue #4.

## User smoke-test feedback (2026-11-21)

A user launched the built prototype with a local MP4 and confirmed video playback.
The progress bar initially filled from its center toward both sides. Anchoring
the fill rectangle at `x: 0px; y: 0px` fixed the visual problem, as confirmed
by the user's subsequent retest.

The user also confirmed **resize and fullscreen work** and reported that
performance and frame pacing **appear good** during use. This establishes
basic feasibility of the Slint/libmpv rendering arrangement and overlay.

Remaining unmeasured production-quality details: actual hardware decode mode,
quantitative dropped-frame and frame-time data, repeatability across GPU
configurations, release binary licensing/packaging, and individually reported
pause/seek behavior. Do not interpret subjective frame pacing as a benchmark.
These are follow-up engineering checks, not blockers for the initial concept.

Visual regression check: at 0%, 25%, 50%, and 100% of clip duration,
verify the blue region begins at the left edge and fills that fraction
of the gray track. Verify that clicking the left and right ends still
seeks to the corresponding time.

## Expected test record

Windows build and libmpv versions; GPU/driver and refresh rate; codec/resolution/frame rate; hwdec mode; presentation mode; 30-second CPU/GPU/memory baseline; dropped/mistimed frame counts; seek, pause, resize, fullscreen outcomes; screenshot of overlaid controls; blockers and fallback decision.
