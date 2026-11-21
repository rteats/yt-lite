# yt-lite

Native Rust YouTube client for Windows 10/11. Work in progress; **not yet a playable application**.

## Extraction feasibility proof ([issue #3](https://github.com/rteats/yt-lite/issues/3))

Two routes expose one typed result: the Rust `rusty_ytdl` library and an optional local `yt-dlp` helper. Neither requires a Google account or browser cookies. This proof does not bundle yt-dlp; packaging is a later ticket.

```sh
cargo test --all-targets
cargo run -- "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --provider rust
cargo run -- "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --provider yt-dlp --yt-dlp yt-dlp
cargo run -- "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --provider auto
```

**Current yt-dlp dependency:** For full YouTube support, yt-dlp needs a JavaScript runtime to solve YouTube challenges. Its [official EJS guide](https://github.com/yt-dlp/yt-dlp/wiki/EJS) recommends Deno (minimum 2.3.0 in the referenced guide), enabled by default. The official Windows `yt-dlp.exe` bundles the required EJS scripts, **but not Deno**. Put `deno.exe` on `PATH` or beside `yt-dlp.exe`, then check `deno --version`. Because the harness passes `--ignore-config`, it cannot rely on user yt-dlp configuration to enable nondefault JS runtimes. A self-contained bundled runtime is tracked by issue #4.

The default output is a **privacy-safe JSON capability summary** containing provider, duration, stream counts, captions and chapters. It intentionally excludes video IDs, stream/caption URLs, thumbnails and channel/title metadata. Each capability distinguishes `available`, `unavailable` and `unknown`. `unknown` means not reported, not necessarily absent. `video_streams` counts any stream containing video, including `muxed_streams` (both audio and video) and `video_only_streams` (no audio). `audio_only_streams` counts audio-only formats. yt-dlp storyboard/image entries are excluded from the playable stream count. Caption counts are track/format entries, not distinct languages.

`--provider auto` (default) falls back to yt-dlp when the Rust extraction route fails or returns no streams. Supply `--yt-dlp PATH` if the helper is not on `PATH`.

Use `--raw-json` **only for private debugging** when signed streaming URLs are necessary; its output must not be posted to issues, logs or public reports. `--summary` explicitly selects the safe/default output.

Both providers successfully resolved one public YouTube video on Windows in a user test (2026-11-21). This does **not** yet establish playback compatibility or reliability across representative video types. Run the remaining Windows checks in [extraction feasibility notes](docs/extraction-feasibility.md) and share only the safe summaries/error codes.

See [approved implementation spec](https://github.com/rteats/yt-lite/blob/docs/grill-round-10/docs/specification.md).
