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

The default output is a **privacy-safe JSON capability summary** containing provider, duration, stream counts, captions and chapters. It intentionally excludes video IDs, stream/caption URLs, thumbnails and channel/title metadata. Each capability distinguishes `available`, `unavailable` and `unknown`. `unknown` means not reported, not necessarily absent.

`--provider auto` (default) falls back to yt-dlp when the Rust extraction route fails or returns no streams. Supply `--yt-dlp PATH` if the helper is not on `PATH`.

Use `--raw-json` **only for private debugging** when signed streaming URLs are necessary; its output must not be posted to issues, logs or public reports. `--summary` explicitly selects the safe/default output.

Live YouTube extraction is **not established by the offline CI**. Run the Windows checks in [extraction feasibility notes](docs/extraction-feasibility.md) and share only the safe summaries/error codes.

See [approved implementation spec](https://github.com/rteats/yt-lite/blob/docs/grill-round-10/docs/specification.md).
