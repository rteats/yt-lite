# yt-lite

Native Rust YouTube client for Windows 10/11. Work in progress; **not yet a playable application**.

## Extraction feasibility proof ([issue #3](https://github.com/rteats/yt-lite/issues/3))

Two routes expose one typed result: a local Rust `rusty_ytdl` library and an optional `yt-dlp` helper. Neither requires a Google account or browser cookies. This proof does not bundle yt-dlp; packaging is a later ticket.

```sh
cargo test --all-targets
cargo run -- "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --provider rust
cargo run -- "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --provider yt-dlp --yt-dlp yt-dlp
```

`--provider auto` (default) retries with yt-dlp if Rust extraction fails or returns no playable stream. Provider outputs report `available`, `unavailable` or `unknown` per capability. `unknown` means not reported, not necessarily absent. No commands are executed while running the offline fixture tests.

**Privacy:** Successful JSON output contains signed streaming URLs. Treat it as sensitive; do not paste it into public issue reports. Failure messages omit raw provider stderr and stream URLs.

See [extraction findings](docs/extraction-feasibility.md) and [approved implementation spec](https://github.com/rteats/yt-lite/blob/docs/grill-round-10/docs/specification.md).
