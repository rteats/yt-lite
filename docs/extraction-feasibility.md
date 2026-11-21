# Issue #3 — extraction feasibility log

## Routes and scope

| Route | Implementation | Metadata | Streams | Captions | Chapters |
| --- | --- | --- | --- | --- | --- |
| `rust` | `rusty_ytdl` 0.7.4, Rust library | Yes | Format/URL list | Not exposed by this crate; `unknown` | Provided when source exposes them |
| `yt-dlp` | External `yt-dlp` command with `--dump-single-json` | Yes | Format/URL list | Manual and automatic tracks | Provided when source exposes them |
| `auto` | Rust, retry whole extraction with yt-dlp on failure or missing streams | Yes | Yes | Depends on winning provider | Depends on winning provider |

All live capabilities depend on upstream public video accessibility. No account/cookies/proxies are configured; restricted, paid, private, geo-blocked and DRM media are not bypassed.

## Deterministic checks

`tests/fixtures/` contains hand-reduced, nonsecret, **synthetic** provider payloads (schema examples, not evidence of successful live requests). `cargo test --all-targets` covers typed results, stream selection metadata, captions, chapters, missing-vs-unknown capabilities, invalid JSON, returned-ID mismatch, and video URL validation. CI runs offline on Ubuntu and Windows.

## Pending live validation (not claimed as complete)

On Windows 10 x64 with current `yt-dlp`, run each provider on:

1. Public single video with captions and chapters.
2. Public single video without chapters.
3. Live stream and Shorts video.
4. Unavailable or access-restricted video (record error codes, **not** signed URLs).

Record date, extractor version, pass/fail, metadata/format count, captions, chapters, runtime dependency, failure type, and whether fallback helped. Verify against actual video UI. Never commit full provider JSON containing signed URLs.

## Dependencies and provenance

- `rusty_ytdl` 0.7.4 is MIT OR Apache-2.0 per docs.rs. Its real extraction compatibility is **unverified**.
- `yt-dlp` is distributed under the Unlicense; recheck the official license and current release before packaging. The harness expects a separately supplied binary, performs no automatic download, and captures only provider JSON.
- All harness source code was written for yt-lite. The fixtures are synthetic.
- Final AGPLv3 licensing and binary redistribution obligations are separate issue #4.

## Known limits

- The Rust route cannot currently expose captions; it returns `unknown`.
- `auto` handles extraction failure/missing streams, not per-capability recovery or expired URLs (issue #22).
- Live extraction, Windows compilation, signed-URL validity, and playback have not been verified in this isolated environment.
- The Slint/libmpv playback proof is issue #2, outside this harness.
